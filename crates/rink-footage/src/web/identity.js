(() => {
  const element = id => document.getElementById(id);
  const video = element('tracking-video');
  const overlay = element('skater-overlay');
  const context = overlay.getContext('2d');
  const status = element('skater-status');
  const slider = element('skater-frame');
  const pick = element('pick-skater');
  const clear = element('clear-skater');
  const suggest = element('suggest-skater');
  let review;

  const label = id => 'S' + String(id).padStart(2, '0');
  const clock = ms => {
    const tenths = Math.floor(ms / 100);
    return Math.floor(tenths / 600) + ':' + String(Math.floor(tenths / 10) % 60).padStart(2, '0') + '.' + tenths % 10;
  };

  async function loadJson(path, options) {
    const response = await fetch(path, options);
    if (!response.ok) {
      const payload = await response.json().catch(() => ({}));
      throw new Error(payload.error ?? 'The Pi could not load or save this selection.');
    }
    return response.json();
  }

  function close() {
    review = undefined;
    context.clearRect(0, 0, overlay.width, overlay.height);
    overlay.classList.remove('picking');
    pick.disabled = true;
    pick.textContent = 'Pick yourself';
    suggest.hidden = clear.hidden = element('skater-picking').hidden = true;
    element('skater-candidates').replaceChildren();
    element('skater-anchors').replaceChildren();
    status.textContent = '';
  }

  async function open(session, clip, base) {
    close();
    const focus = '/footage/api/sessions/' + session.id + '/focus';
    const next = {clip, focus, edit: base.replace(/\/tracks$/, '/focus')};
    review = next;
    status.textContent = 'Loading skater tracks…';
    try {
      const [response, selections] = await Promise.all([fetch(base + '.ndjson'), loadJson(focus)]);
      if (!response.ok) throw new Error('Track data is unavailable for this clip.');
      const records = (await response.text()).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
      if (review !== next) return;
      if (!records.length) throw new Error('There are no tracked frames in this clip.');
      next.records = records;
      next.selections = selections;
      next.hasGear = records.some(frame => frame.tracks.some(track => track.gear));
      [overlay.width, overlay.height] = [records[0].width, records[0].height];
      slider.max = records.length - 1;
      showAnchors(next);
      draw(video.currentTime);
    } catch (error) {
      if (review === next) status.textContent = error.message;
    }
  }

  function anchors(item) {
    return item.selections?.[item.clip.id] ?? [];
  }

  function reference(item) {
    return Object.entries(item.selections).find(([clipId, saved]) =>
      clipId !== item.clip.id && saved.some(anchor => anchor.gear))?.[1].find(anchor => anchor.gear);
  }

  function showAnchors(item) {
    const entries = anchors(item);
    element('skater-anchors').replaceChildren(...entries.map(anchor =>
      button(label(anchor.track_id) + ' · ' + clock(anchor.source_ms), () => {
        video.pause();
        const index = item.records.findIndex(frame => frame.source_ms === anchor.source_ms);
        if (index >= 0) seek(item, index);
      })
    ));
    suggest.hidden = !item.hasGear || !reference(item) || entries.length !== 0;
  }

  function indexAt(records, seconds) {
    const time = records[0].source_ms + Math.round(seconds * 1000);
    let low = 0;
    let high = records.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (records[middle].source_ms <= time) low = middle + 1;
      else high = middle;
    }
    return Math.max(0, low - 1);
  }

  function seek(item, index) {
    const seconds = (item.records[index].source_ms - item.records[0].source_ms) / 1000;
    video.currentTime = seconds + 0.01;
    draw(video.currentTime);
  }

  function button(text, action) {
    const node = document.createElement('button');
    node.type = 'button';
    node.textContent = text;
    node.addEventListener('click', action);
    return node;
  }

  function draw(seconds) {
    const item = review;
    if (!item?.records) return;
    const index = indexAt(item.records, seconds);
    if (item.lastIndex === index) return;
    item.lastIndex = index;
    const frame = item.records[index];
    const saved = anchors(item);
    item.anchor = saved.findLast(entry => entry.source_ms <= frame.source_ms) ?? saved[0];
    const confirmed = frame.tracks.find(track => track.id === item.anchor?.track_id);
    const candidates = item.picking ? frame.tracks.filter(track => !track.predicted) : [];
    const proposal = item.proposal?.source_ms === frame.source_ms ? item.proposal : null;
    item.candidates = candidates;
    slider.value = index;
    element('skater-time').textContent = clock(frame.source_ms);
    pick.disabled = item.saving === true;
    pick.textContent = item.picking ? 'Done picking' : saved.length ? 'Correct track' : 'Pick yourself';
    clear.hidden = !item.anchor || item.picking;
    clear.disabled = item.saving === true;
    overlay.classList.toggle('picking', item.picking === true);
    element('skater-picking').hidden = !item.picking;

    const candidateButtons = candidates.map(track => {
      const offered = proposal?.id === track.id;
      const choice = button(offered ? 'Confirm ' + label(track.id) : label(track.id),
        () => save(item, frame.source_ms, track.id));
      if (offered) choice.dataset.suggested = '';
      choice.disabled = item.saving === true;
      return choice;
    });
    element('skater-candidates').replaceChildren(...candidateButtons);

    status.textContent = item.message ?? (item.picking
      ? candidates.length
        ? proposal ? 'Check the suggested skater, then confirm or choose a different ID.' : 'Tap your skater in the footage or choose their ID.'
        : 'Scrub to a frame where your skater has a solid box.'
      : saved.length
        ? confirmed ? 'You · ' + label(confirmed.id) : 'Your saved track is out of view here.'
        : 'Choose the skater you want to follow in this clip.');

    context.clearRect(0, 0, overlay.width, overlay.height);
    for (const track of candidates) {
      drawBox(track, track.id === proposal?.id ? '#ffd260' : '#b7e5ff', 2, label(track.id), false);
    }
    if (confirmed) {
      drawBox(confirmed, '#ffd260', 3, 'YOU · ' + label(confirmed.id), confirmed.predicted);
    }
  }

  function drawBox(track, color, width, text, dashed) {
    const [left, top, right, bottom] = track.xyxy;
    context.setLineDash(dashed ? [7, 5] : []);
    context.strokeStyle = '#151d24';
    context.lineWidth = width + 2;
    context.strokeRect(left, top, right - left, bottom - top);
    context.strokeStyle = color;
    context.lineWidth = width;
    context.strokeRect(left, top, right - left, bottom - top);
    context.setLineDash([]);
    context.font = 'bold 14px system-ui,sans-serif';
    context.textBaseline = 'middle';
    const tagWidth = context.measureText(text).width + 10;
    const x = Math.max(0, Math.min(left, overlay.width - tagWidth));
    const y = Math.max(0, top - 19);
    context.fillStyle = '#151d24';
    context.fillRect(x, y, tagWidth, 18);
    context.fillStyle = color;
    context.fillText(text, x + 5, y + 9);
  }

  function repaint(item) {
    if (review !== item) return;
    item.lastIndex = undefined;
    draw(video.currentTime);
  }

  async function save(item, source_ms, track_id) {
    if (item.saving) return;
    item.saving = true;
    item.editVersion = (item.editVersion ?? 0) + 1;
    item.message = 'Saving selection…';
    repaint(item);
    try {
      const saved = await loadJson(item.edit, {
        method:'PUT', headers:{'Content-Type':'application/json'}, body:JSON.stringify({source_ms, track_id}),
      });
      if (item !== review) return;
      item.selections[item.clip.id] = saved;
      item.picking = false;
      item.proposal = undefined;
      item.message = undefined;
      showAnchors(item);
      window.dispatchEvent(new Event('skater-selection-saved'));
    } catch (error) {
      item.message = error.message;
    } finally {
      item.saving = false;
      repaint(item);
    }
  }

  pick.addEventListener('click', () => {
    const item = review;
    if (!item?.records) return;
    video.pause();
    item.picking = !item.picking;
    item.proposal = item.message = undefined;
    repaint(item);
  });
  clear.addEventListener('click', () => {
    if (review?.anchor) save(review, review.anchor.source_ms, null);
  });
  slider.addEventListener('input', () => {
    if (review?.records) seek(review, Number(slider.value));
  });
  overlay.addEventListener('click', event => {
    const item = review;
    if (!item?.picking || item.saving || video.seeking) return;
    const bounds = overlay.getBoundingClientRect();
    const x = (event.clientX - bounds.left) * overlay.width / bounds.width;
    const y = (event.clientY - bounds.top) * overlay.height / bounds.height;
    const matches = item.candidates.filter(track => {
      const [l, t, r, b] = track.xyxy;
      return x >= l && x <= r && y >= t && y <= b;
    });
    if (matches.length === 1) save(item, item.records[item.lastIndex].source_ms, matches[0].id);
    else if (matches.length > 1) {
      item.message = 'Those boxes overlap. Choose an ID below the footage.';
      repaint(item);
    }
  });

  function gearDistance(a, b) {
    const weights = [0.40, 0.40, 0.20];
    return weights.reduce((total, weight, zone) => total + weight *
      a[zone].reduce((distance, count, color) => distance + Math.abs(count - b[zone][color]), 0) / 40, 0);
  }

  function findProposal(records, gear) {
    const candidates = new Map();
    for (const frame of records) {
      for (const track of frame.tracks) {
        if (track.predicted || !track.gear) continue;
        if (!candidates.has(track.id)) candidates.set(track.id, []);
        const samples = candidates.get(track.id);
        if (samples.length && frame.source_ms - samples.at(-1).source_ms < 250) continue;
        samples.push({source_ms: frame.source_ms, id: track.id, score: gearDistance(gear, track.gear)});
      }
    }
    const ranking = [...candidates.values()].filter(samples => samples.length >= 3).map(samples => {
      samples.sort((a, b) => a.score - b.score);
      return {best: samples[0], score: (samples[0].score + samples[1].score + samples[2].score) / 3};
    }).sort((a, b) => a.score - b.score);
    const [best, second] = ranking;
    return best && best.score <= 0.35 && (!second || second.score - best.score >= 0.08) ? best.best : null;
  }

  suggest.addEventListener('click', () => {
    const item = review;
    const seed = item?.records ? reference(item) : null;
    if (!seed) return;
    const possible = findProposal(item.records, seed.gear);
    if (!possible) {
      item.message = 'No clear gear match. Pick yourself in this clip.';
      repaint(item);
      return;
    }
    video.pause();
    item.proposal = possible;
    item.picking = true;
    item.message = undefined;
    item.lastIndex = undefined;
    seek(item, item.records.findIndex(frame => frame.source_ms === possible.source_ms));
  });

  for (const event of ['loadedmetadata', 'seeked', 'pause']) {
    video.addEventListener(event, () => draw(video.currentTime));
  }
  video.addEventListener('play', () => {
    if (!review?.picking) return;
    review.picking = false;
    review.message = undefined;
    repaint(review);
  });
  if ('requestVideoFrameCallback' in video) {
    const paintFrame = (_, metadata) => {
      if (!video.seeking) draw(metadata.mediaTime);
      video.requestVideoFrameCallback(paintFrame);
    };
    video.requestVideoFrameCallback(paintFrame);
  } else {
    video.addEventListener('timeupdate', () => draw(video.currentTime));
  }
  window.addEventListener('focus', async () => {
    const item = review;
    if (!item?.records || item.saving) return;
    const version = item.editVersion;
    try {
      const latest = await loadJson(item.focus);
      if (item !== review || item.saving || item.editVersion !== version) return;
      item.selections = latest;
      showAnchors(item);
      repaint(item);
    } catch {}
  });
  window.skaterReview = {open, close};
})();
