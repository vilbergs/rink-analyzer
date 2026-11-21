//! Read the HEVC configuration and expose a video-only view of the source.
//! QuickTime audio and metadata atoms can use layouts the MP4 reader rejects.

use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    ops::Range,
};

use anyhow::{bail, ensure, Context, Result};

#[derive(Clone)]
struct Atom {
    kind: [u8; 4],
    body: Range<u64>,
    kind_offset: u64,
}

fn atoms(file: &mut File, span: Range<u64>) -> Result<Vec<Atom>> {
    let mut result = Vec::new();
    let mut start = span.start;
    while start < span.end {
        file.seek(SeekFrom::Start(start))?;
        if span.end - start < 8 {
            let mut padding = [0; 7];
            file.read_exact(&mut padding[..(span.end - start) as usize])?;
            ensure!(padding == [0; 7], "truncated MOV atom header at byte {start}");
            break;
        }
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let short_size = u32::from_be_bytes(header[..4].try_into().unwrap());
        let (size, header_size) = match short_size {
            0 => (span.end - start, 8),
            1 => {
                ensure!(span.end - start >= 16, "truncated extended MOV atom header");
                let mut extended = [0; 8];
                file.read_exact(&mut extended)?;
                (u64::from_be_bytes(extended), 16)
            }
            value => (u64::from(value), 8),
        };
        ensure!(size >= header_size && size <= span.end - start, "invalid MOV atom size");
        let end = start + size;
        result.push(Atom {
            kind: header[4..].try_into().unwrap(),
            body: start + header_size..end,
            kind_offset: start + 4,
        });
        start = end;
    }
    Ok(result)
}

fn child(file: &mut File, span: Range<u64>, kind: &[u8; 4]) -> Result<Option<Atom>> {
    Ok(atoms(file, span)?.into_iter().find(|atom| &atom.kind == kind))
}

fn hevc_configuration(file: &mut File, track: &Atom) -> Result<Option<Vec<u8>>> {
    let Some(media) = child(file, track.body.clone(), b"mdia")? else {
        return Ok(None);
    };
    let Some(handler) = child(file, media.body.clone(), b"hdlr")? else {
        return Ok(None);
    };
    ensure!(handler.body.end - handler.body.start >= 12, "truncated MOV track handler");
    file.seek(SeekFrom::Start(handler.body.start + 8))?;
    let mut kind = [0; 4];
    file.read_exact(&mut kind)?;
    if &kind != b"vide" {
        return Ok(None);
    }

    let mut span = media.body;
    for kind in [b"minf", b"stbl", b"stsd"] {
        span = child(file, span, kind)?.context("missing video sample table")?.body;
    }
    // Skip the full-box header and entry count to reach the sample entries.
    ensure!(span.end - span.start >= 8, "truncated sample description");
    let entries = atoms(file, span.start + 8..span.end)?;
    let Some(entry) = entries.iter().find(|entry| matches!(&entry.kind, b"hvc1" | b"hev1")) else {
        return Ok(None);
    };
    ensure!(entries.len() == 1, "multiple video sample descriptions are unsupported");
    // A VisualSampleEntry starts with 78 bytes of fixed fields.
    ensure!(entry.body.end - entry.body.start >= 78, "truncated HEVC sample entry");
    let config = child(file, entry.body.start + 78..entry.body.end, b"hvcC")?.context("missing HEVC configuration")?;
    let len = config.body.end - config.body.start;
    ensure!(len <= 1024 * 1024, "oversized HEVC configuration");
    let mut data = vec![0; len as usize];
    file.seek(SeekFrom::Start(config.body.start))?;
    file.read_exact(&mut data)?;
    Ok(Some(data))
}

pub struct VideoOnlyMov {
    file: File,
    skipped_atoms: Vec<u64>,
}

impl VideoOnlyMov {
    pub fn open(mut file: File) -> Result<(Self, Vec<u8>)> {
        let len = file.metadata()?.len();
        let movie = child(&mut file, 0..len, b"moov")?.context("missing movie metadata")?;
        let mut config = None;
        let mut skipped_atoms = Vec::new();
        for atom in atoms(&mut file, movie.body)? {
            match &atom.kind {
                b"udta" | b"meta" => skipped_atoms.push(atom.kind_offset),
                b"trak" => {
                    let track_config = if config.is_none() { hevc_configuration(&mut file, &atom)? } else { None };
                    if let Some(track_config) = track_config {
                        config = Some(track_config);
                        skipped_atoms.extend(
                            atoms(&mut file, atom.body)?
                                .iter()
                                .filter(|child| matches!(&child.kind, b"udta" | b"meta"))
                                .map(|child| child.kind_offset),
                        );
                    } else {
                        skipped_atoms.push(atom.kind_offset);
                    }
                }
                _ => {}
            }
        }
        let Some(config) = config else {
            bail!("no HEVC video track found");
        };
        file.rewind()?;
        Ok((Self { file, skipped_atoms }, config))
    }
}

impl Read for VideoOnlyMov {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let start = self.file.stream_position()?;
        let size = self.file.read(buffer)?;
        // Mask only the atom type so the source and all sample offsets stay intact.
        for &offset in &self.skipped_atoms {
            let low = start.max(offset);
            let high = (start + size as u64).min(offset + 4);
            if low < high {
                buffer[(low - start) as usize..(high - start) as usize]
                    .copy_from_slice(&b"free"[(low - offset) as usize..(high - offset) as usize]);
            }
        }
        Ok(size)
    }
}

impl Seek for VideoOnlyMov {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}
