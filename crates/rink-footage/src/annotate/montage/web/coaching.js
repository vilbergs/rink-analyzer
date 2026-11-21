const notes = {
  IMG_2911: {
    25733: [
      'You accelerate up the camera-side lane while the play moves up the far side. That separation gives the puck carrier a second passing lane.',
      'Keep that separation through the turn. If you drift toward the far-side group too early, one defender can cover both options.',
      'Match the puck carrier’s speed from the wide lane. As the play goes deeper, look inside before you change lanes.',
      ['player', 'reference', 'lane', 'width', 'route'],
    ],
    29133: [
      'You’ve carried speed and kept the near-side outlet available through the transition.',
      'As the far-side group goes deeper, staying on the same wide line can leave your stick farther from a passing lane. A shallow diagonal gives the carrier a better angle without costing much speed.',
      'Take one stride inward and show your blade if a teammate has control and the inside lane is open. Stay wide if someone already fills it. The puck is hard to see in this clip.',
      ['player', 'lane', 'move', 'route', 'ghost'],
    ],
  },
  IMG_2912: {
    37800: [
      'You keep moving through traffic and recover your stride after the skaters come together.',
      'You and the orange skater are nearly level as your routes converge. An earlier adjustment gives you room for your next push before you’re skating in the same space.',
      'Glance toward the boards, then take one small step outside if there’s space. If you’re defending the middle, protect inside position. Possession is unclear in this clip.',
      ['player', 'reference', 'move', 'route', 'ghost'],
    ],
  },
  IMG_2914: {
    24800: [
      'You stay high near the blue line while the other skaters go deeper. That keeps a return pass available and gives you depth if the play turns.',
      'A short scan before you slow down gives you time to find a passing angle. Move along the line so you can offer your blade while keeping the same depth.',
      'Take one lateral stride only if your team has control and another skater can cover high. Hold your depth if you’re the last high option or the puck is contested.',
      ['player', 'reference', 'move', 'route', 'ghost'],
    ],
  },
};

const symbols = {
  player: ['You in the footage', 'Gold ring at your skates.', '<ellipse cx="23" cy="12" rx="14" ry="6" fill="none" stroke="currentColor" stroke-width="2"/>'],
  reference: ['Reference line', 'Blue dashes trace the play, the orange skater’s line, or the blue line.', '<path class="dashed" d="M7 12h32"/>'],
  lane: ['Your wide lane', 'The gold shaded strip shows the camera-side lane to keep open.', '<path d="M8 12h30" opacity=".38" style="stroke-width:11"/><path d="M8 12h30"/>'],
  width: ['Width between lanes', 'The gold double arrow shows the space between your route and the far-side play.', '<path d="M8 12h30m-30 0 5-4m-5 4 5 4m25-4-5-4m5 4-5 4"/>'],
  move: ['Suggested move in the footage', 'Follow the green arrow if the space is open and the situation allows it.', '<path d="M8 12h28m-7-5 8 5-8 5" style="stroke-width:3"/>'],
  route: ['You on the rink', 'The gold outline marks your position traced from the footage.', '<circle cx="23" cy="12" r="7" style="fill:none" stroke="currentColor" stroke-width="2"/>'],
  ghost: ['Suggested position on the rink', 'The green ghost shows where the adjustment would take you.', '<circle cx="23" cy="12" r="7" style="fill:#d8f5e3" stroke="currentColor" stroke-width="2" stroke-dasharray="3 2"/>'],
};

const references = {
  IMG_2911: 'Blue dashes show the far-side play. Keep a passing lane open on your side.',
  IMG_2912: 'Blue dashes show the orange skater’s line as your routes narrow.',
  IMG_2914: 'Blue dashes trace the attacking blue line. The green arrow runs alongside it.',
};

const data = window.coachingTiming;
const element = id => document.getElementById(id);
const error = element('error');
if (!data?.chapters?.length) {
  error.hidden = false;
} else {
  start(data);
}

function start({fps, rinkSize, chapters}) {
  const hostedSession = location.pathname.match(/^\/footage\/room\/([\da-f-]{36})\//i);
  if (hostedSession) {
    const back = element('back-session');
    back.href = '/footage/?session=' + hostedSession[1];
    back.hidden = false;
  }
  const footage = element('footage');
  const rink = element('rink');
  const context = rink.getContext('2d');
  const backdrop = document.createElement('canvas');
  [rink.width, rink.height] = rinkSize;
  [backdrop.width, backdrop.height] = rinkSize;
  const background = backdrop.getContext('2d');
  const slider = element('scrub');
  const play = element('play');
  const lastFrame = chapters.at(-1).endFrame - 1;
  const chapterButtons = new Map();
  let chapter;
  let frameDrawn;
  let reviewShown;
  let markerButtons;
  const detail = element('puck-detail');
  const detailContext = detail.getContext('2d');
  const patch = detailContext.createImageData(detail.width, detail.height);
  let firstPuckFrame;
  element('find-puck').addEventListener('click', () => seek(firstPuckFrame.frame));

  function showPuck(entry) {
    if (!firstPuckFrame) return;
    const pixels = entry.puckDetail;
    if (!pixels) {
      detailContext.clearRect(0, 0, detail.width, detail.height);
      element('puck-status').textContent = 'No tracked position on this frame.';
      return;
    }
    for (let i = 0; i < patch.data.length / 4; i++) {
      const level = parseInt(pixels.slice(i * 2, i * 2 + 2), 16);
      patch.data.set([level, level, level, 255], i * 4);
    }
    detailContext.putImageData(patch, 0, 0);
    element('puck-status').textContent = '3× · Enhanced grayscale';
  }

  const format = ms => {
    const deciseconds = Math.round(Math.max(0, ms) / 100);
    const minutes = String(Math.floor(deciseconds / 600)).padStart(2, '0');
    return minutes + ':' + ((deciseconds % 600) / 10).toFixed(1).padStart(4, '0');
  };
  const button = (text, action) => {
    const control = document.createElement('button');
    control.type = 'button';
    control.textContent = text;
    control.addEventListener('click', action);
    return control;
  };
  const write = (tag, contents, className) => {
    const item = document.createElement(tag);
    item.textContent = contents;
    if (className) item.className = className;
    return item;
  };
  function seek(frame, pause = true) {
    if (pause) footage.pause();
    footage.currentTime = (Math.min(frame, lastFrame) + .1) / fps;
    update(footage.currentTime);
  }
  function atMs(ms) {
    const entry = chapter.rinkFrames[Math.min(Math.round(ms * fps / 1000), chapter.rinkFrames.length - 1)];
    seek(entry.frame);
  }
  function marker(lane, ms, style, title, action = () => atMs(ms)) {
    const control = button('', action);
    control.className = 'marker ' + style;
    control.title = title;
    control.setAttribute('aria-label', title);
    control.style.left = percent(ms);
    lane.append(control);
    return control;
  }
  const percent = ms => Math.min(100, ms / chapter.durationMs * 100) + '%';

  function showChapter(next) {
    chapter = next;
    frameDrawn = null;
    reviewShown = null;
    markerButtons = new Map();
    for (const [item, control] of chapterButtons) control.setAttribute('aria-pressed', String(item === chapter));
    firstPuckFrame = chapter.rinkFrames.find(entry => entry.puckDetail);
    element('puck-panel').hidden = !firstPuckFrame;
    slider.max = chapter.durationMs;
    for (const id of ['ruler', 'events', 'feedback-lane', 'keys', 'moments']) element(id).replaceChildren();
    const tickTimes = [];
    for (let ms = 0; ms < chapter.durationMs - 450; ms += 1000) tickTimes.push(ms);
    tickTimes.push(chapter.durationMs);
    for (const ms of tickTimes) {
      const tick = write('span', ms % 1000 ? (ms / 1000).toFixed(1) : String(ms / 1000), 'tick');
      tick.style.left = percent(ms);
      tick.classList.toggle('last', ms === chapter.durationMs);
      element('ruler').append(tick);
    }
    for (const caption of chapter.captions) {
      marker(element('events'), caption.t_ms, 'caption', caption.text);
    }
    for (const event of chapter.events) {
      marker(element('events'), event.t_ms, 'event', event.note || event.kind.replaceAll('_', ' '));
    }
    for (const ms of chapter.keyframes) {
      marker(element('keys'), ms, 'key', 'Your position at ' + format(ms));
    }
    for (const hold of chapter.holds) {
      const title = format(hold.sourceMs) + '  ' + hold.cue;
      const ms = hold.decision ? chapter.suggestion.tMs : hold.sourceMs - chapter.sourceMs;
      const jump = () => seek(hold.frame);
      const name = hold.decision ? title + ' · ' + chapter.suggestion.principle : title;
      const mark = marker(element('feedback-lane'), ms, hold.decision ? 'decision' : 'freeze', name, jump);
      const control = button(format(hold.sourceMs), jump);
      control.title = title;
      control.setAttribute('aria-label', title);
      element('moments').append(control);
      markerButtons.set(hold, [mark, control]);
    }
    background.clearRect(0, 0, ...rinkSize);
    paint(background, chapter.rinkBase);
  }

  function renderReview(focus) {
    if (focus === reviewShown) return;
    if (!focus) {
      element('review').textContent = 'No freeze frames in this sequence.';
      element('legend').replaceChildren();
      reviewShown = focus;
      return;
    }
    reviewShown = focus;
    for (const [hold, [mark, control]] of markerButtons) {
      mark.setAttribute('aria-current', String(hold === focus));
      control.setAttribute('aria-pressed', String(hold === focus));
    }
    const content = window.coachingFeedback?.[chapter.clip]?.[focus.sourceMs] ?? notes[chapter.clip]?.[focus.sourceMs];
    const review = element('review');
    review.replaceChildren(
      write('p', chapter.clip + ' · ' + format(focus.sourceMs), 'review-time'),
      write('h3', focus.cue, 'review-title'),
    );
    const lines = content
      ? [['What you did well', content[0]], ['What to adjust', content[1]], ['What to try next', content[2]]]
      : [['What to try next', focus.detail]];
    for (const [heading, copy] of lines) {
      const paragraph = write('p', '', 'note');
      paragraph.append(write('strong', heading), document.createTextNode(copy));
      review.append(paragraph);
    }
    const legend = element('legend');
    legend.replaceChildren();
    for (const kind of content?.[3] ?? ['player', 'route', 'ghost']) {
      const [title, detail, glyph] = symbols[kind];
      const row = document.createElement('li');
      const icon = document.createElement('span');
      icon.innerHTML = '<svg class="symbol ' + kind + '" viewBox="0 0 46 23" aria-hidden="true">' + glyph + '</svg>';
      const copy = document.createElement('span');
      copy.append(write('strong', title), write('small', kind === 'reference' ? content?.[4] ?? references[chapter.clip] ?? detail : detail));
      row.append(icon, copy);
      legend.append(row);
    }
  }

  function update(seconds) {
    const frame = Math.min(lastFrame, Math.floor(seconds * fps + .001));
    const next = chapters.find(item => frame < item.endFrame) ?? chapters.at(-1);
    if (chapter !== next) showChapter(next);
    const entry = chapter.rinkFrames.findLast(item => item.frame <= frame);
    const latest = chapter.holds.findLast(hold => hold.frame <= frame);
    const active = latest && frame < latest.frame + latest.frames ? latest : null;
    renderReview(latest ?? chapter.holds[0]);
    const ms = active?.decision ? chapter.suggestion.tMs : entry.ms;
    if (!slider.matches(':active')) slider.value = ms;
    element('playhead').style.left = percent(ms);
    element('clock').textContent = format(ms) + ' / ' + format(chapter.durationMs);
    element('source').textContent = chapter.clip + ' · ' + format(active?.sourceMs ?? chapter.sourceMs + ms);
    const caption = chapter.captions.findLast(item => item.t_ms <= ms);
    const text = caption?.text ?? '';
    if (element('caption').textContent !== text) element('caption').textContent = text;
    if (frameDrawn !== entry) {
      showPuck(entry);
      context.clearRect(0, 0, ...rinkSize);
      context.drawImage(backdrop, 0, 0);
      paint(context, entry.shapes);
      frameDrawn = entry;
      rink.dataset.frame = String(entry.frame);
    }
  }

  for (const item of chapters) {
    const control = button(item.title, () => seek(item.frame));
    chapterButtons.set(item, control);
    element('sequences').append(control);
  }
  if (window.coachingExports?.length) {
    const downloads = element('exports');
    downloads.hidden = false;
    for (const [file, label] of [
      ['coaching-montage.mp4', 'Download montage'],
      ...window.coachingExports.flatMap((item, index) => [
        [item.file, 'Editable rink play ' + (index + 1)],
        [item.file.replace('.rink.json', '.review.json'), 'Feedback ' + (index + 1)],
      ]),
    ]) {
      const link = write('a', label);
      link.href = file;
      link.download = file;
      downloads.append(link);
    }
  }
  for (const track of document.querySelectorAll('.track')) {
    const move = event => {
      const bounds = track.getBoundingClientRect();
      const fraction = Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width));
      atMs(fraction * chapter.durationMs);
    };
    track.addEventListener('pointerdown', event => {
      if (event.button !== 0 || event.target.closest('button')) return;
      track.setPointerCapture(event.pointerId);
      move(event);
    });
    track.addEventListener('pointermove', event => {
      if (track.hasPointerCapture(event.pointerId)) move(event);
    });
  }
  slider.addEventListener('input', () => atMs(Number(slider.value)));
  element('speed').addEventListener('change', event => { footage.playbackRate = Number(event.target.value); });
  function togglePlayback() {
    if (!footage.paused) {
      footage.pause();
      return;
    }
    if (footage.ended) seek(0);
    footage.play().catch(() => { error.hidden = false; });
  }
  play.addEventListener('click', togglePlayback);
  document.addEventListener('keydown', event => {
    if (event.key !== ' ' || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const target = event.target;
    if (target instanceof HTMLElement &&
        (target.isContentEditable || target.closest('button, input:not([type="range"]), select, textarea, summary, a'))) return;
    event.preventDefault();
    if (!event.repeat) togglePlayback();
  });
  footage.addEventListener('play', () => { play.textContent = 'Pause'; });
  footage.addEventListener('pause', () => { play.textContent = 'Play'; });
  footage.addEventListener('ended', () => { play.textContent = 'Replay'; });
  footage.addEventListener('error', () => { error.hidden = false; });
  footage.addEventListener('seeked', () => update(footage.currentTime));
  if ('requestVideoFrameCallback' in footage) {
    const follow = (_, metadata) => {
      if (!footage.seeking) update(metadata.mediaTime);
      footage.requestVideoFrameCallback(follow);
    };
    footage.requestVideoFrameCallback(follow);
  } else {
    footage.addEventListener('timeupdate', () => update(footage.currentTime));
  }
  update(0);
}

function paint(context, shapes) {
  for (const shape of shapes) {
    const color = '#' + shape.color.toString(16).padStart(8, '0');
    if (shape.kind === 'text') {
      context.font = shape.size_px + 'px system-ui, sans-serif';
      context.textAlign = 'center';
      context.textBaseline = 'middle';
      context.fillStyle = color;
      context.fillText(shape.text, ...shape.at);
      continue;
    }
    context.beginPath();
    for (const [x, y] of shape.pts) context.lineTo(x, y);
    if (shape.kind === 'fill') {
      context.closePath();
      context.fillStyle = color;
      context.fill();
    } else {
      if (shape.closed) context.closePath();
      context.lineWidth = shape.width_px;
      context.setLineDash(shape.dash ?? []);
      context.strokeStyle = color;
      context.stroke();
    }
  }
}
