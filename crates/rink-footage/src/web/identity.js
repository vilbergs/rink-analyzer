(() => {
  const element = id => document.getElementById(id);
  const video = element('tracking-video');
  const overlay = element('skater-overlay');
  const context = overlay.getContext('2d');
  const status = element('skater-status');
  const trackingDetails = element('tracking-details');
  const trackCount = element('tracking-track-count');
  const cutSelect = element('tracking-cut');
  const slider = element('skater-frame');
  const pick = element('pick-skater');
  const clear = element('clear-skater');
  const suggest = element('suggest-skater');
  const subject = element('study-player');
  const partner = element('study-partner');
  const ranges = element('study-ranges');
  const alias = element('study-name');
  const team = element('study-team');
  const jersey = element('study-jersey');
  const match = element('identity-match');
  const identityStatus = element('identity-status');
  let review;
  let lastStudy;

  const viewport = element('tracking-viewport');
  const reviewPanel = element('tracking-review');
  const stage = element('tracking-stage');
  const zoom = element('tracking-zoom');
  const pan = element('tracking-pan');
  const fit = element('tracking-reset');
  const play = element('tracking-play');
  let offset = {x:0,y:0};
  let drag;
  let gestureStartScale;

  function viewportScale() { return Number(zoom.value) / 100; }

  function updateViewport() {
    const scale = viewportScale();
    const xLimit = viewport.clientWidth * (scale-1) / 2;
    const yLimit = viewport.clientHeight * (scale-1) / 2;
    offset.x = Math.max(-xLimit,Math.min(xLimit,offset.x));
    offset.y = Math.max(-yLimit,Math.min(yLimit,offset.y));
    stage.style.transform = 'translate(' + offset.x + 'px,' + offset.y + 'px) scale(' + scale + ')';
    element('tracking-scale').textContent = Math.round(Number(zoom.value)) + '%';
    pan.disabled = fit.disabled = scale === 1;
    if (scale === 1) setPan(false);
  }

  function setPan(enabled) {
    pan.setAttribute('aria-pressed',String(enabled));
    viewport.classList.toggle('panning',enabled);
    if (!enabled) endDrag();
  }

  function endDrag() {
    drag = undefined;
    viewport.classList.remove('dragging');
  }

  function resetViewport() {
    gestureStartScale = undefined;
    zoom.value = 100;
    offset = {x:0,y:0};
    updateViewport();
  }

  function zoomAt(scale, event) {
    const previous = viewportScale();
    zoom.value = scale * 100;
    const ratio = viewportScale() / previous;
    const bounds = viewport.getBoundingClientRect();
    const x = event.clientX - (bounds.left + bounds.width / 2);
    const y = event.clientY - (bounds.top + bounds.height / 2);
    // Hold the frame point under the cursor in place as the stage scales.
    offset.x = (Number.isFinite(x) ? x : 0) * (1 - ratio) + offset.x * ratio;
    offset.y = (Number.isFinite(y) ? y : 0) * (1 - ratio) + offset.y * ratio;
    updateViewport();
  }

  zoom.addEventListener('input', updateViewport);
  fit.addEventListener('click', resetViewport);
  const expand = element('tracking-expand');
  const videoFrame = viewport.parentElement;
  expand.hidden = !document.fullscreenEnabled;
  expand.addEventListener('click', async () => {
    try {
      if (document.fullscreenElement === videoFrame) await document.exitFullscreen();
      else {
        await videoFrame.requestFullscreen();
        viewport.focus();
      }
    } catch {
      expand.title = 'Full screen is unavailable';
    }
  });
  document.addEventListener('fullscreenchange', () => {
    expand.setAttribute('aria-pressed', String(document.fullscreenElement === videoFrame));
  });
  viewport.addEventListener('wheel', event => {
    if (zoom.disabled || (!event.ctrlKey && viewportScale() === 1 && gestureStartScale === undefined)) return;
    event.preventDefault();
    if (gestureStartScale !== undefined) return;
    const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.clientHeight : 1;
    if (event.ctrlKey) {
      zoomAt(viewportScale() * Math.exp(-event.deltaY * unit / 200), event);
    } else {
      offset.x -= event.deltaX * unit;
      offset.y -= event.deltaY * unit;
      updateViewport();
    }
  }, {passive:false});
  // Safari delivers trackpad pinches as gesture events with cumulative scale.
  viewport.addEventListener('gesturestart', event => {
    if (zoom.disabled) return;
    event.preventDefault();
    gestureStartScale = viewportScale();
  }, {passive:false});
  viewport.addEventListener('gesturechange', event => {
    if (gestureStartScale === undefined) return;
    event.preventDefault();
    zoomAt(gestureStartScale * event.scale, event);
  }, {passive:false});
  viewport.addEventListener('gestureend', event => {
    if (gestureStartScale === undefined) return;
    event.preventDefault();
    gestureStartScale = undefined;
  }, {passive:false});

  pan.addEventListener('click', () => {
    const enabled = pan.getAttribute('aria-pressed') !== 'true';
    setPan(enabled);
    if (enabled) viewport.focus({preventScroll:true});
  });
  viewport.addEventListener('pointerdown', event => {
    if (!viewport.classList.contains('panning') || drag || event.button !== 0) return;
    event.preventDefault();
    viewport.setPointerCapture(event.pointerId);
    drag = {id:event.pointerId,x:event.clientX,y:event.clientY};
    viewport.classList.add('dragging');
  });
  viewport.addEventListener('pointermove', event => {
    if (drag?.id !== event.pointerId) return;
    offset.x += event.clientX-drag.x;
    offset.y += event.clientY-drag.y;
    drag.x = event.clientX;
    drag.y = event.clientY;
    updateViewport();
  });
  viewport.addEventListener('lostpointercapture', endDrag);
  viewport.addEventListener('keydown', event => {
    if (!viewport.classList.contains('panning')) return;
    const delta = {ArrowLeft:[-24,0],ArrowRight:[24,0],ArrowUp:[0,-24],ArrowDown:[0,24]}[event.key];
    if (!delta) return;
    event.preventDefault();
    offset.x += delta[0];
    offset.y += delta[1];
    updateViewport();
  });
  new ResizeObserver(updateViewport).observe(viewport);

  const fullscreenPlay = element('fullscreen-play');
  fullscreenPlay.addEventListener('click', () => play.click());
  function updatePlay() {
    fullscreenPlay.textContent = play.textContent = video.paused || video.ended ? 'Play' : 'Pause';
  }
  for (const event of ['play','pause','ended','emptied']) video.addEventListener(event,updatePlay);
  document.addEventListener('keydown', event => {
    if (event.key !== ' ' || event.defaultPrevented || event.isComposing || reviewPanel.hidden || play.disabled ||
        event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const target = event.target;
    if (target !== document.body && !reviewPanel.contains(target)) return;
    if (target.isContentEditable || target.closest('button, a, select, textarea, summary, input:not([type=range])')) return;
    event.preventDefault();
    if (!event.repeat) play.click();
  });
  play.addEventListener('click', () => {
    if (video.paused) video.play().catch(() => {
      (element('player-study').hidden ? status : identityStatus).textContent = 'The clip could not play.';
    });
    else video.pause();
  });

  function updateOrientation() {
    const {videoWidth, videoHeight} = video;
    const portrait = videoWidth > 0 && videoWidth < videoHeight;
    reviewPanel.classList.toggle('portrait', portrait);
    if (videoHeight) reviewPanel.style.setProperty('--video-ratio', videoWidth / videoHeight);
    else reviewPanel.style.removeProperty('--video-ratio');
  }
  for (const event of ['loadedmetadata','resize','emptied']) video.addEventListener(event, updateOrientation);

  function showInspector(study) {
    element('review-tools').hidden = !study;
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
    if (review?.referenceVideo) {
      review.referenceVideo.removeAttribute('src');
      review.referenceVideo.load();
    }
    review = undefined;
    trackingDetails.hidden = true;
    playReview.hidden = true;
    playReview.open = false;
    trackCount.textContent = '';
    play.disabled = zoom.disabled = slider.disabled = true;
    resetViewport();
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

  async function open(session, clip, base, moment) {
    close();
    const focus = '/footage/api/sessions/' + session.id + '/focus';
    const identity = focus.replace(/\/focus$/, '/identity');
    const next = {session, clip, focus, identity, edit: base.replace(/\/tracks$/, '/focus')};
    review = next;
    status.textContent = 'Loading skater tracks…';
    try {
      const [response, selections, sessionIdentity] = await Promise.all([
        fetch(base + '.ndjson'), loadJson(focus), loadJson(identity),
      ]);
      if (!response.ok) throw new Error('Track data is unavailable for this clip.');
      const records = (await response.text()).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
      if (review !== next) return;
      if (!records.length) throw new Error('There are no tracked frames in this clip.');
      next.records = records;
      trackingDetails.hidden = false;
      showCameraCuts(records);
      next.run = response.headers.get('x-rink-tracking-run');
      next.identityState = sessionIdentity;
      next.selections = selections;
      next.hasGear = records.some(frame => frame.tracks.some(track => track.gear));
      [overlay.width, overlay.height] = [records[0].width, records[0].height];
      slider.max = records.length - 1;
      play.disabled = zoom.disabled = slider.disabled = false;
      initStudy(next);
      showAnchors(next);
      if (moment) jumpToMoment(next,moment);
      else draw(video.currentTime);
    } catch (error) {
      if (review === next) status.textContent = error.message;
    }
  }

  function anchors(item) {
    return item.selections?.[item.clip.id] ?? [];
  }

  function showCameraCuts(records) {
    const cuts = records.flatMap((frame,index) =>
      index && frame.shot != null && frame.shot !== records[index-1].shot ?
        [new Option(clock(frame.source_ms),String(index))] : []);
    element('tracking-cut-controls').hidden = !cuts.length;
    cutSelect.replaceChildren(new Option('Choose a cut',''),...cuts);
  }
  cutSelect.addEventListener('change', () => {
    if (!review?.records || !cutSelect.value) return;
    video.pause();
    review.stopBefore = null;
    seek(review,Number(cutSelect.value));
    cutSelect.value = '';
  });

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
    if (lastStudy?.session === item.session.id) {
      item.studyId = lastStudy.player;
      item.partnerId = lastStudy.partner;
    } else {
      item.studyId = item.partnerId = null;
    }
    prepareAssignments(item);
    identityStatus.textContent = '';
    match.replaceChildren();
    hideReference(item);
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
    showPlays(item);
    resetPlay(item);
    playReview.hidden = !item.run;
    element('inspector-nav').hidden = false;
    showInspector(true);
  }

  function aliasKey(item,id) { return 'rink.study.' + item.clip.id + '.' + id; }
  element('pick-study').addEventListener('click', () => {
    const item = review;
    if (!item?.players) return;
    video.pause();
    setPan(false);
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
  function prepareAssignments(item) {
    item.assignmentMap = new Map();
    for (const assignment of item.identityState.assignments) {
      const {view} = assignment;
      if (view.clip !== item.clip.id || view.run !== item.run) continue;
      if (!item.assignmentMap.has(view.track_id)) item.assignmentMap.set(view.track_id, []);
      item.assignmentMap.get(view.track_id).push(assignment);
    }
    for (const list of item.assignmentMap.values()) list.sort((a,b) => a.view.source_ms - b.view.source_ms);
  }

  function assignmentAt(item, source_ms, id) {
    return item.assignmentMap.get(id)?.findLast(a => a.view.source_ms <= source_ms);
  }

  function selectedTrack(item, frame, selection, detected = false) {
    return frame.tracks.find(track => (!detected || !track.predicted) && (
      typeof selection === 'number' ? track.id === selection :
      selection && assignmentAt(item, frame.source_ms, track.id)?.player === selection));
  }

  function selectId(value) {
    return /^\d+$/.test(value) ? Number(value) : value || null;
  }

  function studyOptions(item) {
    const players = item.identityState.players;
    subject.replaceChildren(new Option('Select a player',''));
    partner.replaceChildren(new Option('Player alone',''));
    if (anchors(item).length) partner.add(new Option('Your saved skater','you'));
    const options = [
      ...Object.entries(players).map(([id,name]) => [id,name]),
      ...[...item.players.keys()].sort((a,b)=>a-b).map(id => [id,playerLabel(item,id)]),
    ];
    for (const [id,name] of options) {
      subject.add(new Option(name,String(id)));
      if (id !== item.studyId) partner.add(new Option(name,String(id)));
    }
    if (item.studyId != null && !options.some(([id]) => id === item.studyId)) item.studyId = null;
    if (item.partnerId !== 'you' && !options.some(([id]) => id === item.partnerId)) item.partnerId = null;
    subject.value = item.studyId ?? '';
    partner.value = item.partnerId ?? '';
    partner.disabled = item.studyId == null;
    lastStudy = {session:item.session.id, player:typeof item.studyId === 'string' ? item.studyId : null,
      partner:typeof item.partnerId === 'string' ? item.partnerId : null};
    item.controlKey = undefined;
  }

  function companion(item, frame, detected = false) {
    if (item.partnerId !== 'you') return selectedTrack(item, frame, item.partnerId, detected);
    const saved = anchors(item);
    const id = (saved.findLast(a => a.source_ms <= frame.source_ms) ?? saved[0])?.track_id;
    return selectedTrack(item, frame, id, detected);
  }
  function updateRanges(item) {
    item.stopBefore = null;
    const runs = [];
    let last;
    for (let i=0; item.studyId != null && i<item.records.length; i++) {
      const frame = item.records[i];
      const watched = selectedTrack(item, frame, item.studyId, true);
      const other = companion(item, frame, true);
      if (!watched || (item.partnerId != null && (!other || other.id === watched.id))) continue;
      if (!last || last.end !== i-1) { last = {start:i,end:i}; runs.push(last); }
      last.end = i;
    }
    item.runs = runs;
    element('study-note').hidden = item.studyId == null;
    ranges.replaceChildren(...runs.map((run,i) => new Option(
      clock(item.records[run.start].source_ms) + ' – ' + clock(item.records[run.end].source_ms),String(i))));
    element('study-next').disabled = ranges.disabled = !runs.length;
    element('study-status').textContent = item.studyId == null ? 'Select a track to review its appearances.' :
      !runs.length ? 'No visible appearances for this selection in this clip.' :
      item.partnerId == null ? 'Watch the strides and scans before the player changes direction.' :
      'Watch when the gap opens or closes. Where could you move to offer support or cover the lane?';
  }
  function showNotes(item) {
    item.notes.sort((a,b) => a.index-b.index);
    const shared = item.identityState.moments.map(moment => sharedMoment(item, moment));
    element('study-notes').replaceChildren(...shared, ...item.notes.map(note => {
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
  element('study-note').addEventListener('submit', async event => {
    event.preventDefault();
    const item = review;
    const field = element('note-text');
    if (item?.studyId == null || !field.value.trim()) return;
    const index = indexAt(item.records,video.currentTime);
    const frame = item.records[index];
    const track = selectedTrack(item, frame, item.studyId, true);
    const player = track && assignmentAt(item, frame.source_ms, track.id)?.player;
    if (player) {
      const other = companion(item, frame, true);
      const compare = other && assignmentAt(item, frame.source_ms, other.id)?.player;
      if (await saveIdentity(item, {action:'save_moment', frame:frameKey(item, frame, track), player,
        compare:other && compare !== player ? frameKey(item, frame, other) : null, text:field.value.trim()})) field.value = '';
      return;
    }
    if (typeof item.studyId === 'string') {
      identityStatus.textContent = 'Seek to a frame where the player is visible to save this moment.';
      return;
    }
    const note = {player:item.studyId,partner:item.partnerId,index,text:field.value.trim()};
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
    item.studyId = selectId(subject.value);
    if (item.studyId == null || item.partnerId === item.studyId) item.partnerId = null;
    studyOptions(item); updateRanges(item); repaint(item);
  });
  partner.addEventListener('change', () => {
    if (!review?.players) return;
    review.partnerId = selectId(partner.value);
    studyOptions(review); updateRanges(review); repaint(review);
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

  const frameKey = (item,frame,track) => ({
    clip:item.clip.id, run:item.run, source_ms:frame.source_ms, track_id:track.id,
  });
  const sameFrame = (a,b) => a && b && a.clip === b.clip && a.run === b.run &&
    a.source_ms === b.source_ms && a.track_id === b.track_id;

  function studyTarget(item) {
    if (!item?.records) return {};
    const frame = item.records[indexAt(item.records,video.currentTime)];
    const track = selectedTrack(item,frame,item.studyId,true);
    if (!track || !item.run || item.identityState.runs[item.clip.id] !== item.run) return {};
    return {frame:frameKey(item,frame,track), player:assignmentAt(item,frame.source_ms,track.id)?.player};
  }

  function referenceFor(item,player) {
    return item.offers?.candidates.find(c => c.player === player)?.reference ??
      item.identityState.assignments.findLast(a => a.player === player &&
        !sameFrame(a.view,item.target) && stateAssignment(item,a.view) === player)?.view;
  }

  function hideReference(item) {
    element('identity-reference').hidden = true;
    item.referenceRequest = undefined;
  }

  function showReference(item) {
    const view = referenceFor(item,match.value);
    if (!view || element('player-study').hidden) { hideReference(item); return; }
    if (sameFrame(view,item.referenceRequest)) return;
    item.referenceRequest = view;
    const figure = element('identity-reference');
    const canvas = element('identity-reference-frame');
    const clip = item.session.clips.find(c => c.id === view.clip);
    const name = item.identityState.players[match.value];
    figure.hidden = false;
    canvas.hidden = true;
    const caption = figure.querySelector('figcaption');
    caption.textContent = 'Loading confirmed view of ' + name + '…';
    const media = document.createElement('video');
    media.preload = 'auto';
    media.muted = true;
    if (item.referenceVideo) {
      item.referenceVideo.removeAttribute('src');
      item.referenceVideo.load();
    }
    item.referenceVideo = media;
    const valid = () => item === review && item.referenceRequest === view;
    const time = clock(view.source_ms);
    const captionText = name + ' · ' + (clip?.name ?? 'Clip') + ' · ' + time;
    media.addEventListener('error', () => {
      if (valid()) caption.textContent = captionText + ' · Frame unavailable';
    });
    media.addEventListener('seeked', () => {
      if (!valid()) return;
      [canvas.width,canvas.height] = view.size;
      const ctx = canvas.getContext('2d');
      ctx.drawImage(media,0,0,canvas.width,canvas.height);
      const [l,t,r,b] = view.xyxy;
      ctx.strokeStyle = '#ffd260';
      ctx.lineWidth = Math.max(3,canvas.width/160);
      ctx.strokeRect(l,t,r-l,b-t);
      canvas.hidden = false;
      caption.textContent = captionText;
    }, {once:true});
    const base = '/footage/api/sessions/' + item.session.id + '/clips/' + view.clip + '/tracks';
    // The target and reference can come from clips with different source offsets.
    const offset = view.clip === item.clip.id ? Promise.resolve(item.records[0].source_ms) :
      referenceOffset(item,base,view.run);
    Promise.all([offset, new Promise((resolve,reject) => {
      media.addEventListener('loadeddata',resolve,{once:true});
      media.addEventListener('error',reject,{once:true});
      media.src = base + '.mp4';
    })]).then(([first]) => {
      if (valid()) media.currentTime = Math.max(0,(view.source_ms-first)/1000 + 0.01);
    }).catch(() => {
      if (valid()) caption.textContent = captionText + ' · Frame unavailable';
    });
  }

  async function referenceOffset(item,base,run) {
    item.offsets ??= new Map();
    if (item.offsets.has(run)) return item.offsets.get(run);
    const response = await fetch(base + '.ndjson',{cache:'no-store'});
    if (!response.ok || response.headers.get('x-rink-tracking-run') !== run) throw new Error('Tracking changed.');
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let line = '';
    try {
      while (!line.includes('\n')) {
        const {value,done} = await reader.read();
        if (done) break;
        line += decoder.decode(value,{stream:true});
      }
      const time = JSON.parse(line.split('\n',1)[0]).source_ms;
      item.offsets.set(run,time);
      return time;
    } finally {
      await reader.cancel();
    }
  }

  function identityControls(item,frame) {
    const track = selectedTrack(item,frame,item.studyId,true);
    const editable = track && item.run && item.identityState.runs[item.clip.id] === item.run;
    const target = editable ? frameKey(item,frame,track) : null;
    const player = editable ? assignmentAt(item,frame.source_ms,track.id)?.player : null;
    if (!sameFrame(item.offers?.frame,target)) item.offers = undefined;
    const candidates = item.offers?.candidates ?? [];
    const detailsKey = JSON.stringify([item.studyId, target?.track_id, player, item.identityState.revision]);
    const key = JSON.stringify([detailsKey, item.offers?.clear, candidates.map(c => c.player)]);
    if (key !== item.controlKey) {
      item.controlKey = key;
      const names = item.identityState.players;
      const previous = match.value;
      const offered = new Set(candidates.map(c => c.player));
      const manual = Object.entries(names).filter(([id]) => !offered.has(id));
      match.replaceChildren(new Option('Select a confirmed player',''),
        ...candidates.map(c => new Option((item.offers.clear ? 'Suggested · ' : 'Possible · ') + names[c.player],c.player)),
        ...manual.map(([id,name]) => new Option(name,id)));
      if (offered.size && item.offers.clear) match.value = item.offers.clear;
      else if (!offered.size && Object.hasOwn(names,previous)) match.value = previous;
    }
    if (detailsKey !== item.detailsKey) {
      item.detailsKey = detailsKey;
      alias.value = item.identityState.players[player] ?? item.players.get(item.studyId)?.name ?? '';
      const details = item.identityState.player_details[player];
      team.value = details?.team ?? '';
      jersey.value = details?.jersey ?? '';
      const teams = new Set(Object.values(item.identityState.player_details).map(data => data.team).filter(Boolean));
      element('study-teams').replaceChildren(...Array.from(teams, name => new Option(name)));
    }
    item.target = target;
    const ready = Boolean(editable && !item.identitySaving && video.paused);
    alias.disabled = team.disabled = jersey.disabled = !ready;
    match.disabled = !ready || match.options.length < 2;
    element('identity-label').disabled = !ready;
    element('identity-rename').disabled = !ready || !player;
    element('identity-suggest').disabled = !ready || !Object.keys(item.identityState.players).length;
    element('identity-undo').disabled = item.identitySaving || !item.identityState.undo;
    element('identity-link').disabled = !ready || !match.value;
    element('identity-reject').disabled = !ready || (!player && !candidates.some(c => c.player === match.value));
    element('identity-unsure').hidden = !item.offers;
    element('identity-replay').disabled = !target;
    if (ready && match.value) showReference(item);
    else hideReference(item);
  }

  function refreshIdentity(item,state) {
    const changed = state.revision !== item.identityState.revision ||
      JSON.stringify(state.runs) !== JSON.stringify(item.identityState.runs);
    item.identityState = state;
    prepareAssignments(item);
    if (changed) {
      item.offers = undefined;
      studyOptions(item);
    }
    showNotes(item);
    showPlays(item);
    updateRanges(item);
    repaint(item);
  }

  const jsonPost = payload => ({
    method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload),
  });

  async function saveIdentity(item,action) {
    if (item.identitySaving) return false;
    item.identitySaving = true;
    const payload = {...action, id:crypto.randomUUID(), revision:item.identityState.revision};
    repaint(item);
    try {
      const submit = () => loadJson(item.identity,jsonPost(payload));
      const state = await submit().catch(error => {
        if (error instanceof TypeError) return submit();
        throw error;
      });
      if (review !== item) return false;
      if (action.action === 'assign') {
        item.studyId = action.player;
        if (item.partnerId === action.player) item.partnerId = null;
      } else if (action.action === 'undo' && item.target) {
        item.studyId = stateAssignment({...item,identityState:state},item.target) ?? item.target.track_id;
      }
      refreshIdentity(item,state);
      window.dispatchEvent(new Event('player-identity-saved'));
      identityStatus.textContent = action.action === 'undo' ? 'Edit undone.' :
        action.action === 'reject' ? 'Match rejected for this frame.' : 'Saved.';
      return true;
    } catch (error) {
      const state = await loadJson(item.identity).catch(() => null);
      if (review === item) {
        if (state) refreshIdentity(item,state);
        identityStatus.textContent = error.message;
      }
      return false;
    } finally {
      item.identitySaving = false;
      repaint(item);
    }
  }

  match.addEventListener('change', () => { if (review?.records) repaint(review); });
  function jerseyLabel(details) {
    if (!details?.jersey) return '';
    const prefix = [...details.team].slice(0,38-details.jersey.length).join('').trimEnd();
    return [prefix,'#'+details.jersey].filter(Boolean).join(' ');
  }

  function playerDetails(item, player) {
    const details = {team:team.value.trim(),jersey:jersey.value.trim()};
    const explicitName = alias.value.trim();
    const fallback = jerseyLabel(details);
    const previousFallback = jerseyLabel(item.identityState.player_details[player]);
    const name = explicitName && explicitName !== previousFallback ? explicitName : fallback;
    if (!/^[0-9]{0,2}$/.test(details.jersey)) {
      element('study-player-details').open = true;
      jersey.focus();
      identityStatus.textContent = 'Use one or two digits for the jersey number, or leave it blank.';
      return null;
    }
    if (!name) {
      alias.focus();
      identityStatus.textContent = 'Enter a name or jersey number.';
      return null;
    }
    return {name,details};
  }

  element('identity-label').addEventListener('click', () => {
    const item = review;
    const {frame} = studyTarget(item);
    if (!frame) return;
    const fields = playerDetails(item);
    if (!fields) return;
    saveIdentity(item,{action:'assign', frame, player:crypto.randomUUID(), ...fields,
      shown:item.offers?.candidates.map(c => c.player) ?? []});
  });
  element('identity-rename').addEventListener('click', () => {
    const {frame,player} = studyTarget(review);
    if (!frame || !player) return;
    const fields = playerDetails(review,player);
    if (fields) saveIdentity(review,{action:'rename',frame,player,...fields});
  });
  element('identity-link').addEventListener('click', () => {
    const item = review;
    const {frame} = studyTarget(item);
    if (frame && match.value) saveIdentity(item,{action:'assign',frame,player:match.value,name:null,
      shown:item.offers?.candidates.map(c => c.player) ?? []});
  });
  element('identity-reject').addEventListener('click', async () => {
    const item = review;
    const {frame} = studyTarget(item);
    if (!frame) return;
    const shown = item.offers?.candidates.map(c => c.player) ?? [];
    if (shown.includes(match.value)) {
      if (await saveIdentity(item,{action:'reject',frame,player:match.value,shown}) && review === item) {
        requestSuggestions(item,frame);
      }
    } else {
      alias.value = '';
      alias.focus();
      identityStatus.textContent = 'Link a different confirmed player or enter a new label for this frame onward.';
    }
  });
  element('identity-undo').addEventListener('click', () => {
    if (review?.identityState.undo) saveIdentity(review,{action:'undo',target:review.identityState.undo});
  });
  element('identity-unsure').addEventListener('click', () => {
    review.offers = undefined;
    match.value = '';
    identityStatus.textContent = 'Identity left open.';
    repaint(review);
  });
  element('identity-suggest').addEventListener('click', () => {
    const {frame} = studyTarget(review);
    if (frame) requestSuggestions(review,frame);
  });

  async function requestSuggestions(item,frame) {
    const request = item.suggestionRequest = {};
    try {
      const result = await loadJson(item.identity + '/suggestions',jsonPost(frame));
      if (review !== item || request !== item.suggestionRequest || !sameFrame(frame,studyTarget(item).frame)) return;
      if (result.revision !== item.identityState.revision) {
        const latest = await loadJson(item.identity);
        if (review !== item || request !== item.suggestionRequest || latest.revision < item.identityState.revision) return;
        refreshIdentity(item,latest);
        identityStatus.textContent = 'Player study changed. Find matches again.';
        return;
      }
      item.offers = {...result,frame};
      identityStatus.textContent = result.clear ? 'Check the confirmed view before linking.' :
        result.candidates.length ? 'No clear match. Review these players or leave the identity open.' :
        'No appearance match for this frame. You can still link a player manually.';
      repaint(item);
    } catch (error) {
      if (review === item && request === item.suggestionRequest) identityStatus.textContent = error.message;
    }
  }

  element('identity-replay').addEventListener('click', () => {
    const item = review;
    const boundary = item.lastIndex;
    const at = item.records[boundary].source_ms;
    const start = item.records.findIndex(frame => frame.source_ms >= at-1000);
    const end = item.records.findIndex(frame => frame.source_ms > at+1000);
    video.pause();
    seek(item,start);
    item.stopBefore = end < 0 ? item.records.length : end;
    video.play().catch(() => { item.stopBefore = null; });
  });

  function stateAssignment(item,view) {
    if (item.identityState.runs[view.clip] !== view.run) return null;
    let best;
    for (const assignment of item.identityState.assignments) {
      const other = assignment.view;
      if (other.clip === view.clip && other.run === view.run && other.track_id === view.track_id &&
          other.source_ms <= view.source_ms && (!best || other.source_ms >= best.view.source_ms)) best = assignment;
    }
    return best?.player ?? null;
  }

  function momentChanged(item,moment) {
    return stateAssignment(item,moment.view) !== moment.player ||
      (moment.compare && stateAssignment(item,moment.compare.view) !== moment.compare.player);
  }

  function jumpToMoment(item,moment) {
    item.studyId = moment.player;
    item.partnerId = moment.compare?.player ?? moment.compare?.view.track_id ?? null;
    studyOptions(item);
    updateRanges(item);
    video.pause();
    item.lastIndex = undefined;
    seek(item,indexAt(item.records,(moment.view.source_ms-item.records[0].source_ms)/1000));
    if (momentChanged(item,moment)) identityStatus.textContent = 'The identity assignment at this saved moment has changed.';
  }

  function sharedMoment(item,moment) {
    const row = document.createElement('li');
    const clip = item.session.clips.find(clip => clip.id === moment.view.clip);
    const player = item.identityState.players[moment.player] ?? label(moment.view.track_id);
    const other = moment.compare &&
      (item.identityState.players[moment.compare.player] ?? label(moment.compare.view.track_id));
    const jump = button(clock(moment.view.source_ms) + ' · ' + player + (other ? ' + ' + other : '') +
      (clip && clip.id !== item.clip.id ? ' · ' + clip.name : ''), () => {
        if (moment.view.clip === item.clip.id) jumpToMoment(item,moment);
        else window.dispatchEvent(new CustomEvent('study-moment-open',{detail:moment}));
      });
    const copy = document.createElement('p');
    copy.textContent = moment.text;
    const remove = button('Remove', () => saveIdentity(item,{action:'remove_moment',target:moment.id}));
    row.append(jump,remove,copy);
    if (momentChanged(item,moment)) {
      const changed = document.createElement('p');
      changed.textContent = 'Identity assignment changed';
      row.append(changed);
    }
    return row;
  }

  const playReview = element('play-review');
  const playForm = element('play-form');
  const playPrimary = element('play-primary');
  const playSecondary = element('play-secondary');
  const playStatus = element('play-status');

  function setPlayLabels(item, play) {
    const choices = item.identityState.play_labels;
    playPrimary.replaceChildren(new Option('Choose a coaching label',''),
      ...choices.map(label => new Option(label.name,label.id)));
    playPrimary.value = play?.primary ?? '';
    updateSecondary(item,play?.secondary);
  }

  function updateSecondary(item, value) {
    playSecondary.replaceChildren(new Option('None',''), ...item.identityState.play_labels
      .filter(label => label.id !== playPrimary.value).map(label => new Option(label.name,label.id)));
    if ([...playSecondary.options].some(option => option.value === value)) playSecondary.value = value;
    for (const field of [playPrimary,playSecondary]) {
      const choice = item.identityState.play_labels.find(label => label.id === field.value);
      element(field.id + '-description').textContent = choice?.description ?? '';
    }
  }

  function resetPlay(item, play) {
    item.playDraft = {id:play?.id,start:play?.start_ms,end:play?.end_ms};
    setPlayLabels(item,play);
    element('play-cue').value = play?.cue ?? '';
    element('play-attach').checked = play?.track != null;
    element('play-save').textContent = play ? 'Update play at this frame' : 'Save play at this frame';
    element('play-cancel').hidden = !play;
    playStatus.textContent = '';
    updatePlayRange(item);
  }

  function playBounds(item, decision) {
    const first = item.records[0].source_ms;
    const at = ms => item.records[indexAt(item.records,(ms-first)/1000)].source_ms;
    return {
      start_ms:item.playDraft.start ?? at(decision - 2500),
      end_ms:item.playDraft.end ?? at(decision + 1500),
    };
  }

  function updatePlayRange(item) {
    if (!item.playDraft || !playReview.open) return;
    const index = indexAt(item.records,video.currentTime);
    const decision = item.records[index].source_ms;
    const {start_ms,end_ms} = playBounds(item,decision);
    element('play-range').textContent = clock(start_ms) + ' – ' + clock(end_ms) + ' · Decision ' + clock(decision);
  }

  playReview.addEventListener('toggle', () => { if (review?.records) updatePlayRange(review); });
  playPrimary.addEventListener('change', () => updateSecondary(review,playSecondary.value));
  playSecondary.addEventListener('change', () => updateSecondary(review,playSecondary.value));
  for (const edge of ['start','end']) {
    element('play-' + edge).addEventListener('click', () => {
      const item = review;
      video.pause();
      item.playDraft[edge] = item.records[indexAt(item.records,video.currentTime)].source_ms;
      updatePlayRange(item);
    });
  }
  element('play-cancel').addEventListener('click', () => resetPlay(review));
  playForm.addEventListener('submit', async event => {
    event.preventDefault();
    const item = review;
    if (!item?.playDraft || item.identitySaving) return;
    video.pause();
    const frame = item.records[indexAt(item.records,video.currentTime)];
    const attach = element('play-attach').checked;
    const track = attach && selectedTrack(item,frame,item.studyId,true);
    if (attach && !track) {
      playStatus.textContent = 'Select a detected skater at the decision frame or clear the checkbox.';
      return;
    }
    const play = {
      id:item.playDraft.id ?? crypto.randomUUID(), clip:item.clip.id, run:item.run,
      ...playBounds(item,frame.source_ms), decision_ms:frame.source_ms,
      primary:playPrimary.value, secondary:playSecondary.value || null,
      cue:element('play-cue').value.trim(), track:track?.id ?? null,
    };
    if (await saveIdentity(item,{action:'save_play',play})) {
      resetPlay(item);
      playStatus.textContent = 'Play saved.';
    } else if (review === item) {
      playStatus.textContent = identityStatus.textContent;
    }
  });

  function playIndex(item,ms) {
    return indexAt(item.records,(ms-item.records[0].source_ms)/1000);
  }

  function openPlay(item,play,replay = false) {
    video.pause();
    if (play.player) {
      item.studyId = play.player.player ?? play.track;
      studyOptions(item);
      updateRanges(item);
    }
    item.stopBefore = null;
    seek(item,playIndex(item,replay ? play.start_ms : play.decision_ms));
    if (replay) {
      item.stopBefore = playIndex(item,play.end_ms) + 1;
      video.play().catch(() => { item.stopBefore = null; });
    }
  }

  function showPlays(item) {
    const choices = item.identityState.play_labels;
    element('study-plays').replaceChildren(...item.identityState.plays.filter(play => play.clip === item.clip.id).map(play => {
      const row = document.createElement('li');
      const title = document.createElement('strong');
      title.textContent = [play.primary,play.secondary].filter(Boolean)
        .map(id => choices.find(label => label.id === id)?.name ?? id).join(' · ');
      const decision = button(clock(play.decision_ms),() => openPlay(item,play));
      const replay = button('Replay',() => openPlay(item,play,true));
      const edit = button('Edit',() => {
        openPlay(item,play);
        playReview.open = true;
        resetPlay(item,play);
        playPrimary.focus();
      });
      const remove = button('Remove',async () => {
        if (await saveIdentity(item,{action:'remove_play',target:play.id}) && item.playDraft.id === play.id) resetPlay(item);
      });
      const actions = document.createElement('div');
      actions.className = 'skater-actions';
      actions.append(decision,replay,edit,remove);
      row.append(title,actions);
      if (play.cue) {
        const cue = document.createElement('p');
        cue.textContent = play.cue;
        row.append(cue);
      }
      if (play.run !== item.run || (play.player && stateAssignment(item,play.player.view) !== play.player.player)) {
        const changed = document.createElement('p');
        changed.textContent = 'Tracking or player assignment changed at this play.';
        row.append(changed);
      }
      return row;
    }));
  }

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
    const afterLast = item.stopBefore === item.records.length &&
      seconds * 1000 > item.records.at(-1).source_ms - item.records[0].source_ms + 50;
    if (item.stopBefore != null && (index >= item.stopBefore || afterLast)) {
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
    identityControls(item, frame);
    const candidates = item.picking || item.studyPicking ? frame.tracks.filter(track => !track.predicted) : [];
    const proposal = item.proposal?.source_ms === frame.source_ms ? item.proposal : null;
    item.candidates = candidates;
    slider.value = index;
    updatePlayRange(item);
    element('skater-time').textContent = clock(frame.source_ms);
    const predicted = frame.tracks.filter(track => track.predicted).length;
    trackCount.textContent = clock(frame.source_ms) + ' · ' +
      (frame.tracks.length - predicted) + ' detected · ' + predicted + ' predicted';
    pick.disabled = item.saving === true;
    pick.textContent = item.picking ? 'Done picking' : saved.length ? 'Correct track' : 'Pick yourself';
    clear.hidden = !item.anchor || item.picking;
    clear.disabled = item.saving === true;
    overlay.classList.toggle('picking', Boolean(item.picking || !element('player-study').hidden));
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
    item.tags = new Map();
    const baseTracks = frame.interactive ? frame.tracks : candidates;
    for (const track of baseTracks) {
      drawBox(track, track.id === proposal?.id ? '#ffd260' : '#b7e5ff', 2, label(track.id), track.predicted);
    }
    if (!item.picking && item.studyId != null) {
      const watched = selectedTrack(item, frame, item.studyId, true);
      const comparison = companion(item, frame, true);
      const other = comparison?.id === watched?.id ? null : comparison;
      if (watched && other && !watched.predicted && !other.predicted) {
        context.beginPath();
        for (const [i,t] of [watched,other].entries()) {
          const [left,,right,y] = t.xyxy;
          if (i === 0) context.moveTo((left+right)/2,y); else context.lineTo((left+right)/2,y);
        }
        context.setLineDash([5,4]); context.strokeStyle='#b0f4dd'; context.lineWidth=2; context.stroke(); context.setLineDash([]);
      }
      for (const [t,color] of [[watched,'#a9f2c7'],[other,'#c9baff']]) {
        if (t && t.id !== confirmed?.id) drawBox(t,color,3,item.identityState.players[assignmentAt(item,frame.source_ms,t.id)?.player] || item.players.get(t.id)?.name || label(t.id),t.predicted);
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
    review.tags.set(track.id, [x, y, x + tagWidth, y + 18]);
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
    setPan(false);
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
    if (!item?.records || (!item.picking && element('player-study').hidden) || item.saving || video.seeking) return;
    const bounds = overlay.getBoundingClientRect();
    const x = (event.clientX - bounds.left) * overlay.width / bounds.width;
    const y = (event.clientY - bounds.top) * overlay.height / bounds.height;
    const frame = item.records[item.lastIndex];
    const contains = ([l,t,r,b]) => x >= l && x <= r && y >= t && y <= b;
    const detected = frame.tracks.filter(track => !track.predicted);
    const tags = detected.filter(track => {
      const tag = item.tags.get(track.id);
      if (tag && contains(tag)) return true;
      if (frame.interactive) return false;
      // Older previews have the original renderer's ID tags burned into the frame.
      const left = Math.min(Math.round(track.xyxy[0]), overlay.width - 48);
      const top = Math.max(Math.round(track.xyxy[1]) - 18, 54);
      return contains([left, top, left + Math.max(32,label(track.id).length * 10 + 6), top + 17]);
    });
    const matches = tags.length ? tags : detected.filter(track => contains(track.xyxy));
    if (!item.picking) {
      if (matches.length === 1) {
        video.pause();
        const track = matches[0];
        subject.value = assignmentAt(item,frame.source_ms,track.id)?.player ?? String(track.id);
        subject.dispatchEvent(new Event('change'));
      } else if (matches.length > 1) {
        video.pause();
        element('study-status').textContent = 'Those skaters overlap. Choose a player from the list.';
        subject.focus({preventScroll:true});
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
    video.addEventListener(event, () => { if (review?.records) repaint(review); });
  }
  video.addEventListener('play', () => {
    if (!review?.records) return;
    review.studyPicking = false;
    review.picking = false;
    review.message = undefined;
    if (review.offers) identityStatus.textContent = '';
    review.offers = undefined;
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
      const [latest, state] = await Promise.all([loadJson(item.focus), loadJson(item.identity)]);
      if (item !== review || item.saving || item.identitySaving || item.editVersion !== version) return;
      item.selections = latest;
      if (state.revision >= item.identityState.revision) refreshIdentity(item,state);
      showAnchors(item);
      repaint(item);
    } catch {}
  });
  window.skaterReview = {open, close};
})();
