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
  const subject = element('study-player');
  const partner = element('study-partner');
  const ranges = element('study-ranges');
  const alias = element('study-name');
  let review;

  function showInspector(study) {
    element('player-study').hidden = !study;
    element('self-panel').hidden = study;
    element('show-study').setAttribute('aria-pressed', String(study));
    element('show-self').setAttribute('aria-pressed', String(!study));
    if (review?.players) {
      review.picking = review.studyPicking = false;
      repaint(review);
    }
  }
  element('show-study').addEventListener('click', () => showInspector(true));
  element('show-self').addEventListener('click', () => showInspector(false));

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
    slider.disabled = true;
    slider.value = 0;
    element('skater-time').textContent = '0:00.0';
    element('inspector-nav').hidden = true;
    showInspector(false);
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
      slider.disabled = false;
      initStudy(next);
      showAnchors(next);
      draw(video.currentTime);
    } catch (error) {
      if (review === next) status.textContent = error.message;
    }
  }

  function anchors(item) {
    return item.selections?.[item.clip.id] ?? [];
  }

  function initStudy(item) {
    item.players = new Map();
    for (const frame of item.records) {
      for (const track of frame.tracks) {
        if (track.predicted) continue;
        if (!item.players.has(track.id)) item.players.set(track.id, {colors:[0,0,0,0,0]});
        const player = item.players.get(track.id);
        track.gear?.[1]?.forEach((count, i) => player.colors[i] += count);
      }
    }
    for (const [id, player] of item.players) {
      const peak = Math.max(...player.colors);
      const total = player.colors.reduce((sum,n) => sum+n,0);
      if (peak > total * 0.55) player.jersey = ['red/orange','blue','dark','light',null][player.colors.indexOf(peak)];
      try { player.name = localStorage.getItem(aliasKey(item,id)) ?? ''; } catch { player.name = ''; }
    }
    item.studyId = null;
    item.partnerId = null;
    item.notes = [];
    try {
      const prefix = 'rink.study-note.' + item.clip.id + '.';
      for (const key of Object.keys(localStorage).filter(k => k.startsWith(prefix))) {
        try {
          const note = JSON.parse(localStorage.getItem(key));
          if (item.players.has(note.player) && Number.isInteger(note.index) && item.records[note.index] &&
              typeof note.text === 'string') item.notes.push({...note,key});
        } catch {}
      }
    } catch {}
    element('note-text').value = '';
    showNotes(item);
    element('inspector-nav').hidden = false;
    showInspector(true);
  }

  function aliasKey(item,id) { return 'rink.study.' + item.clip.id + '.' + id; }
  element('pick-study').addEventListener('click', () => {
    const item = review;
    if (!item?.players) return;
    video.pause();
    item.picking = false;
    item.studyPicking = !item.studyPicking;
    element('study-status').textContent = item.studyPicking ? 'Tap a solid box. Use the player list if boxes overlap.' : '';
    if (!item.studyPicking) updateRanges(item);
    repaint(item);
  });
  function playerLabel(item,id) {
    const player = item.players.get(id);
    return [label(id),player.name || (player.jersey && player.jersey + ' jersey')].filter(Boolean).join(' · ');
  }
  function studyOptions(item) {
    subject.replaceChildren(new Option('Select a player',''));
    partner.replaceChildren(new Option('Player alone',''));
    if (anchors(item).length) partner.add(new Option('Your saved skater','you'));
    for (const id of [...item.players.keys()].sort((a,b)=>a-b)) {
      subject.add(new Option(playerLabel(item,id),String(id)));
      if (id !== item.studyId) partner.add(new Option(playerLabel(item,id),String(id)));
    }
    subject.value = item.studyId ?? '';
    partner.value = item.partnerId ?? '';
    partner.disabled = alias.disabled = item.studyId == null;
    alias.value = item.players.get(item.studyId)?.name ?? '';
  }
  function companion(item, frame) {
    if (item.partnerId !== 'you') return item.partnerId;
    const saved = anchors(item);
    return (saved.findLast(a => a.source_ms <= frame.source_ms) ?? saved[0])?.track_id;
  }
  function updateRanges(item) {
    item.stopBefore = null;
    const runs = [];
    let last;
    for (let i=0; item.studyId != null && i<item.records.length; i++) {
      const frame = item.records[i];
      const other = companion(item,frame);
      const seen = id => frame.tracks.some(t => t.id === id && !t.predicted);
      if (!seen(item.studyId) || (item.partnerId != null && (other === item.studyId || !seen(other)))) continue;
      if (!last || last.end !== i-1) { last = {start:i,end:i}; runs.push(last); }
      last.end = i;
    }
    item.runs = runs;
    element('study-note').hidden = item.studyId == null;
    ranges.replaceChildren(...runs.map((run,i) => new Option(
      clock(item.records[run.start].source_ms) + ' – ' + clock(item.records[run.end].source_ms),String(i))));
    element('study-next').disabled = ranges.disabled = !runs.length;
    element('study-status').textContent = item.studyId == null ? 'Select a track to review its appearances.' :
      !runs.length ? 'No frames show both selected tracks together.' :
      item.partnerId == null ? 'Watch the strides and scans before the player changes direction.' :
      'Watch when the gap opens or closes. Where could you move to offer support or cover the lane?';
  }
  function showNotes(item) {
    item.notes.sort((a,b) => a.index-b.index);
    element('study-notes').replaceChildren(...item.notes.map(note => {
      const row = document.createElement('li');
      const frame = item.records[note.index];
      const jump = button(clock(frame.source_ms) + ' · ' + label(note.player), () => {
        item.studyId = note.player;
        item.partnerId = note.partner === 'you' && !anchors(item).length ? null : note.partner;
        studyOptions(item); updateRanges(item);
        video.pause(); item.lastIndex = undefined; seek(item,note.index);
      });
      const copy = document.createElement('p');
      copy.textContent = note.text;
      const remove = button('Remove', () => {
        try { localStorage.removeItem(note.key); }
        catch { element('study-status').textContent = 'Could not remove the saved moment.'; return; }
        item.notes = item.notes.filter(n => n !== note);
        showNotes(item);
      });
      row.append(jump,remove,copy);
      return row;
    }));
  }
  element('note-text').addEventListener('focus', () => video.pause());
  element('study-note').addEventListener('submit', event => {
    event.preventDefault();
    const item = review;
    const field = element('note-text');
    if (item?.studyId == null || !field.value.trim()) return;
    const note = {player:item.studyId,partner:item.partnerId,index:indexAt(item.records,video.currentTime),text:field.value.trim()};
    try {
      const key = 'rink.study-note.' + item.clip.id + '.' + crypto.randomUUID();
      localStorage.setItem(key, JSON.stringify(note));
      item.notes.push({...note,key});
      field.value = '';
      showNotes(item);
    } catch { element('study-status').textContent = 'Could not save this moment on this device.'; }
  });
  subject.addEventListener('change', () => {
    const item = review;
    if (!item?.players) return;
    item.studyPicking = false;
    item.studyId = subject.value ? Number(subject.value) : null;
    if (item.studyId == null || item.partnerId === item.studyId) item.partnerId = null;
    studyOptions(item); updateRanges(item); repaint(item);
  });
  partner.addEventListener('change', () => {
    if (!review?.players) return;
    review.partnerId = partner.value === 'you' ? 'you' : partner.value ? Number(partner.value) : null;
    updateRanges(review); repaint(review);
  });
  alias.addEventListener('change', () => {
    const item = review;
    if (item?.studyId == null) return;
    const name = alias.value.trim();
    item.players.get(item.studyId).name = name;
    studyOptions(item); repaint(item);
    try { localStorage.setItem(aliasKey(item,item.studyId),name); }
    catch { element('study-status').textContent = 'Player label is available until you leave this clip.'; }
  });
  element('study-next').addEventListener('click', () => {
    const item = review;
    const run = item?.runs?.[Number(ranges.value)];
    if (!run) return;
    video.pause();
    seek(item,run.start);
    item.stopBefore = run.end+1;
    video.play().catch(() => {
      if (review !== item) return;
      item.stopBefore = null;
      element('study-status').textContent = 'Press Play in the video to review this appearance.';
    });
  });

  function reference(item) {
    return Object.entries(item.selections).find(([clipId, saved]) =>
      clipId !== item.clip.id && saved.some(anchor => anchor.gear))?.[1].find(anchor => anchor.gear);
  }

  function showAnchors(item) {
    const entries = anchors(item);
    if (item.players) {
      if (!entries.length && item.partnerId === 'you') item.partnerId = null;
      studyOptions(item);
      updateRanges(item);
    }
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
    if (item.stopBefore != null && index >= item.stopBefore) {
      const end = item.stopBefore - 1;
      item.stopBefore = null;
      video.pause(); seek(item,end); return;
    }
    if (item.lastIndex === index) return;
    item.lastIndex = index;
    const frame = item.records[index];
    const saved = anchors(item);
    item.anchor = saved.findLast(entry => entry.source_ms <= frame.source_ms) ?? saved[0];
    const confirmed = frame.tracks.find(track => track.id === item.anchor?.track_id);
    const candidates = item.picking || item.studyPicking ? frame.tracks.filter(track => !track.predicted) : [];
    const proposal = item.proposal?.source_ms === frame.source_ms ? item.proposal : null;
    item.candidates = candidates;
    slider.value = index;
    element('skater-time').textContent = clock(frame.source_ms);
    pick.disabled = item.saving === true;
    pick.textContent = item.picking ? 'Done picking' : saved.length ? 'Correct track' : 'Pick yourself';
    clear.hidden = !item.anchor || item.picking;
    clear.disabled = item.saving === true;
    overlay.classList.toggle('picking', Boolean(item.picking || item.studyPicking));
    element('pick-study').setAttribute('aria-pressed', String(Boolean(item.studyPicking)));
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
    if (!item.picking && item.studyId != null) {
      const watched = frame.tracks.find(t => t.id === item.studyId);
      const other = frame.tracks.find(t => t.id === companion(item,frame) && t.id !== item.studyId);
      if (watched && other && !watched.predicted && !other.predicted) {
        context.beginPath();
        for (const [i,t] of [watched,other].entries()) {
          const [left,,right,y] = t.xyxy;
          if (i === 0) context.moveTo((left+right)/2,y); else context.lineTo((left+right)/2,y);
        }
        context.setLineDash([5,4]); context.strokeStyle='#b0f4dd'; context.lineWidth=2; context.stroke(); context.setLineDash([]);
      }
      for (const [t,color] of [[watched,'#a9f2c7'],[other,'#c9baff']]) {
        if (t && t.id !== confirmed?.id) drawBox(t,color,3,item.players.get(t.id)?.name || label(t.id),t.predicted);
      }
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
    item.studyPicking = false;
    item.picking = !item.picking;
    item.proposal = item.message = undefined;
    repaint(item);
  });
  clear.addEventListener('click', () => {
    if (review?.anchor) save(review, review.anchor.source_ms, null);
  });
  slider.addEventListener('input', () => {
    if (review?.records) { review.stopBefore = null; video.pause(); seek(review, Number(slider.value)); }
  });
  overlay.addEventListener('click', event => {
    const item = review;
    if (!(item?.picking || item?.studyPicking) || item.saving || video.seeking) return;
    const bounds = overlay.getBoundingClientRect();
    const x = (event.clientX - bounds.left) * overlay.width / bounds.width;
    const y = (event.clientY - bounds.top) * overlay.height / bounds.height;
    const matches = item.candidates.filter(track => {
      const [l, t, r, b] = track.xyxy;
      return x >= l && x <= r && y >= t && y <= b;
    });
    if (item.studyPicking) {
      if (matches.length === 1) {
        subject.value = String(matches[0].id);
        subject.dispatchEvent(new Event('change'));
      }
      return;
    }
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
    if (!(review?.picking || review?.studyPicking)) return;
    review.studyPicking = false;
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
