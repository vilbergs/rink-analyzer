//! YOLOv8 detection run straight from an Ultralytics `.pt` checkpoint.
//!
//! Ultralytics pickles the whole `DetectionModel` object rather than a state dict, so the
//! tensors are collected by walking the module tree. Batch norms are folded into their convs
//! at load time. The graph is the stock `yolov8.yaml`; widths and repeat counts come from the
//! checkpoint's tensor shapes, so n/s/m/l/x scales all load.
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use anyhow::{bail, ensure, Context, Result};
use candle_core::{
    pickle::{Object, Stack, TensorInfo},
    DType, Device, Tensor, D,
};
use image::{imageops::FilterType, RgbImage};

const REG_MAX: usize = 16;

/// One box in source-image pixels.
pub struct Detection {
    pub class: usize,
    pub score: f32,
    pub xyxy: [f32; 4],
}

struct Weights {
    tensors: HashMap<String, Tensor>,
    eps: f64,
    device: Device,
}

impl Weights {
    fn get(&self, name: &str) -> Result<Tensor> {
        let tensor = self.tensors.get(name).with_context(|| format!("checkpoint has no tensor {name}"))?;
        Ok(tensor.to_dtype(DType::F32)?.to_device(&self.device)?)
    }

    fn has(&self, name: &str) -> bool {
        self.tensors.contains_key(name)
    }
}

/// Conv2d with its batch norm folded in, followed by SiLU unless `plain`.
struct Conv {
    weight: Tensor,
    bias: Tensor,
    stride: usize,
    padding: usize,
    act: bool,
}

impl Conv {
    fn fused(w: &Weights, prefix: &str, stride: usize) -> Result<Self> {
        let weight = w.get(&format!("{prefix}.conv.weight"))?;
        let gamma = w.get(&format!("{prefix}.bn.weight"))?;
        let beta = w.get(&format!("{prefix}.bn.bias"))?;
        let mean = w.get(&format!("{prefix}.bn.running_mean"))?;
        let var = w.get(&format!("{prefix}.bn.running_var"))?;
        let scale = gamma.broadcast_div(&(var + w.eps)?.sqrt()?)?;
        let out = scale.dim(0)?;
        let padding = weight.dim(2)? / 2;
        Ok(Self {
            weight: weight.broadcast_mul(&scale.reshape((out, 1, 1, 1))?)?,
            bias: (beta - mean.mul(&scale)?)?,
            stride,
            padding,
            act: true,
        })
    }

    fn plain(w: &Weights, prefix: &str) -> Result<Self> {
        Ok(Self {
            weight: w.get(&format!("{prefix}.weight"))?,
            bias: w.get(&format!("{prefix}.bias"))?,
            stride: 1,
            padding: 0,
            act: false,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let y = x.conv2d(&self.weight, self.padding, self.stride, 1, 1)?;
        let y = y.broadcast_add(&self.bias.reshape((1, (), 1, 1))?)?;
        Ok(if self.act { y.silu()? } else { y })
    }
}

struct C2f {
    cv1: Conv,
    cv2: Conv,
    blocks: Vec<(Conv, Conv)>,
    shortcut: bool,
}

impl C2f {
    fn new(w: &Weights, prefix: &str, shortcut: bool) -> Result<Self> {
        let blocks = (0..)
            .take_while(|i| w.has(&format!("{prefix}.m.{i}.cv1.conv.weight")))
            .map(|i| {
                Ok((
                    Conv::fused(w, &format!("{prefix}.m.{i}.cv1"), 1)?,
                    Conv::fused(w, &format!("{prefix}.m.{i}.cv2"), 1)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(!blocks.is_empty(), "{prefix} has no bottlenecks");
        Ok(Self {
            cv1: Conv::fused(w, &format!("{prefix}.cv1"), 1)?,
            cv2: Conv::fused(w, &format!("{prefix}.cv2"), 1)?,
            blocks,
            shortcut,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let mut ys = self.cv1.forward(x)?.chunk(2, 1)?;
        for (a, b) in &self.blocks {
            let last = ys.last().expect("chunk yields two halves");
            let y = b.forward(&a.forward(last)?)?;
            ys.push(if self.shortcut { (y + last)? } else { y });
        }
        self.cv2.forward(&Tensor::cat(&ys, 1)?)
    }
}

struct Sppf {
    cv1: Conv,
    cv2: Conv,
}

impl Sppf {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        // A 5x5 stride-1 max pool with edge replication equals PyTorch's -inf padding:
        // every padded value repeats an edge value that is already inside the window.
        let pool = |t: &Tensor| -> Result<Tensor> {
            Ok(t.pad_with_same(2, 2, 2)?.pad_with_same(3, 2, 2)?.max_pool2d_with_stride(5, 1)?)
        };
        let a = self.cv1.forward(x)?;
        let b = pool(&a)?;
        let c = pool(&b)?;
        let d = pool(&c)?;
        self.cv2.forward(&Tensor::cat(&[a, b, c, d], 1)?)
    }
}

pub struct Yolo {
    convs: HashMap<usize, Conv>,
    c2fs: HashMap<usize, C2f>,
    sppf: Sppf,
    boxes: Vec<[Conv; 3]>,
    scores: Vec<[Conv; 3]>,
    pub names: Vec<String>,
    device: Device,
}

impl Yolo {
    pub fn load(path: &Path, device: Device) -> Result<Self> {
        let (weights, names) = read_checkpoint(path, device.clone())?;
        let w = &weights;
        let m = |i: usize| format!("model.model.{i}");
        let mut convs = HashMap::new();
        for (i, stride) in [(0, 2), (1, 2), (3, 2), (5, 2), (7, 2), (16, 2), (19, 2)] {
            convs.insert(i, Conv::fused(w, &m(i), stride)?);
        }
        let mut c2fs = HashMap::new();
        for (i, shortcut) in
            [(2, true), (4, true), (6, true), (8, true), (12, false), (15, false), (18, false), (21, false)]
        {
            c2fs.insert(i, C2f::new(w, &m(i), shortcut)?);
        }
        let sppf = Sppf {
            cv1: Conv::fused(w, &format!("{}.cv1", m(9)), 1)?,
            cv2: Conv::fused(w, &format!("{}.cv2", m(9)), 1)?,
        };
        let head = |branch: &str, level: usize| -> Result<[Conv; 3]> {
            let p = format!("{}.{branch}.{level}", m(22));
            Ok([
                Conv::fused(w, &format!("{p}.0"), 1)?,
                Conv::fused(w, &format!("{p}.1"), 1)?,
                Conv::plain(w, &format!("{p}.2"))?,
            ])
        };
        let boxes = (0..3).map(|l| head("cv2", l)).collect::<Result<Vec<_>>>()?;
        let scores = (0..3).map(|l| head("cv3", l)).collect::<Result<Vec<_>>>()?;
        ensure!(scores[0][2].weight.dim(0)? == names.len(), "class count does not match the checkpoint's names");
        Ok(Self { convs, c2fs, sppf, boxes, scores, names, device })
    }

    /// Letterbox to `size` (a multiple of 32), run the network, and return boxes in source pixels.
    pub fn detect(&self, image: &RgbImage, size: u32, confidence: &dyn Fn(usize) -> f32) -> Result<Vec<Detection>> {
        ensure!(size % 32 == 0, "input size must be a multiple of 32");
        let (width, height) = image.dimensions();
        let ratio = (size as f32 / width as f32).min(size as f32 / height as f32);
        let (w, h) = ((width as f32 * ratio).round() as u32, (height as f32 * ratio).round() as u32);
        let (left, top) = ((size - w) / 2, (size - h) / 2);
        let mut canvas = RgbImage::from_pixel(size, size, image::Rgb([114, 114, 114]));
        image::imageops::overlay(
            &mut canvas,
            &image::imageops::resize(image, w, h, FilterType::Triangle),
            left.into(),
            top.into(),
        );
        let input = Tensor::from_vec(canvas.into_raw(), (size as usize, size as usize, 3), &self.device)?
            .permute((2, 0, 1))?
            .unsqueeze(0)?
            .to_dtype(DType::F32)?
            .affine(1.0 / 255.0, 0.0)?;
        let output = self.forward(&input)?.squeeze(0)?.t()?.to_vec2::<f32>()?; // (anchors, 4 + classes)

        let mut by_class: HashMap<usize, Vec<Detection>> = HashMap::new();
        for row in output {
            let (class, score) =
                row[4..]
                    .iter()
                    .copied()
                    .enumerate()
                    .fold((0, f32::MIN), |best, (c, s)| if s > best.1 { (c, s) } else { best });
            if score < confidence(class) {
                continue;
            }
            let unbox = |v: f32, offset: u32, limit: u32| ((v - offset as f32) / ratio).clamp(0.0, limit as f32);
            let xyxy = [
                unbox(row[0], left, width),
                unbox(row[1], top, height),
                unbox(row[2], left, width),
                unbox(row[3], top, height),
            ];
            by_class.entry(class).or_default().push(Detection { class, score, xyxy });
        }
        let mut kept: Vec<Detection> = by_class.into_values().flat_map(|found| nms(found, 0.5)).collect();
        kept.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(kept)
    }

    /// Returns (1, 4 + classes, anchors): xyxy in input pixels, then class probabilities.
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let conv = |i: usize, t: &Tensor| self.convs[&i].forward(t);
        let c2f = |i: usize, t: &Tensor| self.c2fs[&i].forward(t);
        let up = |t: &Tensor| -> Result<Tensor> { Ok(t.upsample_nearest2d(t.dim(2)? * 2, t.dim(3)? * 2)?) };

        let x = conv(1, &conv(0, x)?)?;
        let p3 = c2f(4, &conv(3, &c2f(2, &x)?)?)?;
        let p4 = c2f(6, &conv(5, &p3)?)?;
        let p5 = self.sppf.forward(&c2f(8, &conv(7, &p4)?)?)?;
        let h4 = c2f(12, &Tensor::cat(&[up(&p5)?, p4], 1)?)?;
        let o3 = c2f(15, &Tensor::cat(&[up(&h4)?, p3], 1)?)?;
        let o4 = c2f(18, &Tensor::cat(&[conv(16, &o3)?, h4], 1)?)?;
        let o5 = c2f(21, &Tensor::cat(&[conv(19, &o4)?, p5], 1)?)?;

        let bins = Tensor::arange(0f32, REG_MAX as f32, &self.device)?.reshape((1, 1, REG_MAX, 1))?;
        let mut levels = Vec::new();
        for (level, (feature, stride)) in [(o3, 8f64), (o4, 16.0), (o5, 32.0)].into_iter().enumerate() {
            let (rows, cols) = (feature.dim(2)?, feature.dim(3)?);
            let run =
                |convs: &[Conv; 3]| -> Result<Tensor> { convs.iter().try_fold(feature.clone(), |t, c| c.forward(&t)) };
            let dist = run(&self.boxes[level])?.reshape((1, 4, REG_MAX, rows * cols))?;
            let dist = softmax(&dist, 2)?.broadcast_mul(&bins)?.sum(2)?; // (1, 4, anchors): left, top, right, bottom
            let xs = Tensor::arange(0f32, cols as f32, &self.device)?.affine(1.0, 0.5)?.repeat(rows)?;
            let ys = Tensor::arange(0f32, rows as f32, &self.device)?
                .affine(1.0, 0.5)?
                .reshape((rows, 1))?
                .repeat((1, cols))?
                .flatten_all()?;
            let anchor = Tensor::stack(&[xs, ys], 0)?.unsqueeze(0)?; // (1, 2, anchors)
            let low = anchor.sub(&dist.narrow(1, 0, 2)?)?;
            let high = anchor.add(&dist.narrow(1, 2, 2)?)?;
            let xyxy = Tensor::cat(&[low, high], 1)?.affine(stride, 0.0)?;
            let probs = sigmoid(&run(&self.scores[level])?.reshape((1, self.names.len(), rows * cols))?)?;
            levels.push(Tensor::cat(&[xyxy, probs], 1)?);
        }
        Ok(Tensor::cat(&levels, D::Minus1)?)
    }
}

fn softmax(x: &Tensor, dim: usize) -> Result<Tensor> {
    let e = x.broadcast_sub(&x.max_keepdim(dim)?)?.exp()?;
    Ok(e.broadcast_div(&e.sum_keepdim(dim)?)?)
}

fn sigmoid(x: &Tensor) -> Result<Tensor> {
    Ok((x.neg()?.exp()? + 1.0)?.recip()?)
}

fn nms(mut found: Vec<Detection>, threshold: f32) -> Vec<Detection> {
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for d in found {
        if kept.iter().all(|k| iou(&k.xyxy, &d.xyxy) <= threshold) {
            kept.push(d);
        }
    }
    kept
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let inter = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let area = |r: &[f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    inter / (area(a) + area(b) - inter + 1e-9)
}

/// Read every tensor plus `names` and the batch-norm epsilon out of the pickled module tree.
fn read_checkpoint(path: &Path, device: Device) -> Result<(Weights, Vec<String>)> {
    let mut zip =
        zip::ZipArchive::new(BufReader::new(File::open(path).with_context(|| format!("opening {}", path.display()))?))?;
    let pickle = zip
        .file_names()
        .filter_map(Result::ok)
        .find(|n| n.ends_with("/data.pkl"))
        .context("not a PyTorch checkpoint")?
        .into_owned();
    let data_dir = format!("{}/data", pickle.trim_end_matches("/data.pkl"));
    let mut bytes = Vec::new();
    zip.by_name(&pickle)?.read_to_end(&mut bytes)?;
    let mut stack = Stack::empty();
    stack.read_loop(&mut BufReader::new(&bytes[..]))?;
    let root = stack.finalize()?;

    let mut walk = Walk { data_dir: Path::new(&data_dir), infos: Vec::new(), names: Vec::new(), eps: None };
    walk.visit(&root, "", 0);
    ensure!(!walk.infos.is_empty(), "no tensors found in {}", path.display());
    ensure!(!walk.names.is_empty(), "no class names found in {}", path.display());

    let mut tensors = HashMap::new();
    for info in walk.infos {
        // Only cached buffers (Detect anchors/strides) are strided; the network rebuilds those, and a
        // weight that is missing here fails by name when the network asks for it.
        if !info.layout.is_contiguous() {
            continue;
        }
        let mut raw = Vec::new();
        zip.by_name(&info.path)?.read_to_end(&mut raw)?;
        let size = info.dtype.size_in_bytes();
        let start = info.layout.start_offset() * size;
        let end = start + info.layout.shape().elem_count() * size;
        let tensor = Tensor::from_raw_buffer(&raw[start..end], info.dtype, info.layout.dims(), &Device::Cpu)?;
        tensors.insert(info.name, tensor);
    }
    Ok((Weights { tensors, eps: walk.eps.unwrap_or(1e-3), device }, walk.names))
}

struct Walk<'a> {
    data_dir: &'a Path,
    infos: Vec<TensorInfo>,
    names: Vec<String>,
    eps: Option<f64>,
}

impl Walk<'_> {
    fn visit(&mut self, obj: &Object, path: &str, depth: usize) {
        if depth > 64 {
            return;
        }
        if let Ok(Some(info)) = obj.clone().into_tensor_info(Object::Unicode(path.to_owned()), self.data_dir) {
            self.infos.push(info);
            return;
        }
        match obj {
            Object::Dict(items) => {
                for (key, value) in items {
                    let key = match key {
                        Object::Unicode(s) => s.clone(),
                        Object::Int(i) => i.to_string(),
                        _ => continue,
                    };
                    match (key.as_str(), value) {
                        ("names", Object::Dict(names)) if path == "model" => self.names = class_names(names),
                        ("eps", Object::Float(eps)) if self.eps.is_none() => self.eps = Some(*eps),
                        ("_modules" | "_parameters" | "_buffers", _) => self.visit(value, path, depth + 1),
                        _ => self.visit(value, &if path.is_empty() { key } else { format!("{path}.{key}") }, depth + 1),
                    }
                }
            }
            Object::Build { callable, args } | Object::Reduce { callable, args } => {
                self.visit(callable, path, depth + 1);
                self.visit(args, path, depth + 1);
            }
            Object::Tuple(items) | Object::List(items) => {
                items.iter().for_each(|item| self.visit(item, path, depth + 1))
            }
            _ => {}
        }
    }
}

fn class_names(items: &[(Object, Object)]) -> Vec<String> {
    let mut named: Vec<(i32, String)> = items
        .iter()
        .filter_map(|(k, v)| match (k, v) {
            (Object::Int(i), Object::Unicode(name)) => Some((*i, name.clone())),
            _ => None,
        })
        .collect();
    named.sort();
    named.into_iter().map(|(_, name)| name).collect()
}

pub fn device() -> Result<Device> {
    let device = Device::cuda_if_available(0)?;
    if cfg!(feature = "yolo-cuda") && !device.is_cuda() {
        bail!("built for CUDA but no CUDA device is available");
    }
    Ok(device)
}
