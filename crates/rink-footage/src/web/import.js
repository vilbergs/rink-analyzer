const element = id => document.getElementById(id);
const api = '/footage/api/sessions';
const drop = element('drop');
const input = element('files');
const progress = element('progress');
const submit = element('submit');
const status = element('status');
let files = [];
let activeSession;
let activeClip;
let coachingRequest = 0;
let trackingPoll;
let busy = false;
let recentRequest = 0;

const uuid = /^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i;
const write = (tag, value) => {
  const item = document.createElement(tag);
  item.textContent = value;
  return item;
};
const date = ms => new Intl.DateTimeFormat(undefined, {dateStyle: 'medium', timeStyle: 'short'}).format(ms);

async function loadRecent() {
  const request = ++recentRequest;
  const empty = element('empty');
  try {
    const response = await fetch(api);
    if (!response.ok) throw new Error('Recent sessions could not be loaded.');
    const recent = await response.json();
    if (request !== recentRequest) return;
    empty.hidden = recent.length > 0;
    empty.textContent = 'No saved sessions yet.';
    element('recent').replaceChildren(...recent.map(session => {
      const row = document.createElement('li');
      const link = write('a', session.name);
      link.href = '#' + session.id;
      const count = session.clip_count + (session.clip_count === 1 ? ' clip' : ' clips');
      row.append(link, document.createTextNode(count + ' · ' + date(session.created_ms)));
      return row;
    }));
  } catch {
    if (request !== recentRequest) return;
    empty.hidden = false;
    empty.textContent = 'Recent sessions could not be loaded.';
  }
}

function showSession(session) {
  clearTimeout(trackingPoll);
  resetTracking();
  activeSession = session;
  element('session').hidden = false;
  element('session-detail').textContent =
    session.clips.length + (session.clips.length === 1 ? ' clip · ' : ' clips · ') + date(session.created_ms);
  element('clips').replaceChildren(...session.clips.map(clip => {
    const card = document.createElement('article');
    card.className = 'clip';
    const image = document.createElement('img');
    image.src = api + '/' + session.id + '/clips/' + clip.id + '/preview';
    image.alt = 'First frame from ' + clip.name;
    image.loading = 'lazy';
    const seconds = Math.floor(clip.duration_ms / 1000);
    const progress = write('span', 'Checking tracking status…');
    progress.className = 'track-status';
    progress.id = 'track-status-' + clip.id;
    const review = write('button', 'Review skater IDs');
    review.type = 'button';
    review.hidden = true;
    review.id = 'track-review-' + clip.id;
    review.addEventListener('click', () => openTracking(session, clip));
    const coaching = write('a', 'Coaching Film Room');
    coaching.className = 'clip-coaching';
    coaching.id = 'coaching-' + clip.id;
    coaching.hidden = true;
    card.append(image, write('strong', clip.name), write('span',
      Math.floor(seconds / 60) + ':' + String(seconds % 60).padStart(2, '0')), progress, review, coaching);
    return card;
  }));
  pollTracking(session);
  refreshCoaching(session);
}

const labels = {
  queued: 'Queued for tracking',
  processing: 'Tracking skaters on Furulid…',
  ready: 'Skater IDs ready',
  failed: 'Tracking could not finish',
  disabled: 'Tracking unavailable',
};

async function pollTracking(session) {
  let pending = true;
  try {
    const response = await fetch(api + '/' + session.id + '/tracking');
    if (response.status === 404) return;
    if (!response.ok) throw new Error('Could not load tracking status.');
    const states = await response.json();
    if (activeSession !== session) return;
    pending = states.some(item => ['queued', 'processing'].includes(item.tracking));
    for (const item of states) {
      element('track-status-' + item.clip).textContent = labels[item.tracking];
      element('track-review-' + item.clip).hidden = item.tracking !== 'ready';
    }
  } catch {
    if (activeSession !== session) return;
    for (const clip of session.clips) element('track-status-' + clip.id).textContent = 'Checking tracking status…';
  }
  if (pending && activeSession === session) trackingPoll = setTimeout(() => pollTracking(session), 3000);
}

function resetTracking() {
  activeClip = undefined;
  element('coaching-step').hidden = true;
  window.skaterReview.close();
  const video = element('tracking-video');
  video.pause();
  video.removeAttribute('src');
  video.load();
  element('tracking-review').hidden = true;
}

function openTracking(session, clip) {
  activeClip = clip.id;
  const video = element('tracking-video');
  const base = api + '/' + session.id + '/clips/' + clip.id + '/tracks';
  video.src = base + '.mp4';
  element('tracking-data').href = base + '.ndjson';
  element('tracking-title').textContent = clip.name;
  element('tracking-review').hidden = false;
  window.skaterReview.open(session, clip, base);
  refreshCoaching(session);
  video.focus();
  video.play().catch(() => {});
}

async function refreshCoaching(session) {
  if (!session) return;
  const request = ++coachingRequest;
  try {
    const response = await fetch(api + '/' + session.id + '/coaching');
    if (!response.ok) return;
    const rooms = await response.json();
    if (session !== activeSession || request !== coachingRequest) return;
    for (const [clip, room] of Object.entries(rooms)) {
      const link = element('coaching-' + clip);
      link.hidden = !room.url;
      if (room.url) link.href = room.url;
    }
    const active = rooms[activeClip];
    const step = element('coaching-step');
    const link = element('coaching-room');
    step.hidden = !active || active.state === 'choose';
    link.hidden = !active?.url;
    if (active?.url) link.href = active.url;
    element('coaching-state').textContent = ({
      ready: 'Your reviewed sequences, synchronized rink replay, and montage are ready.',
      review: 'Skater selection saved. This clip needs rink alignment and coaching review.',
      changed: 'Your selection changed. The coaching review needs to be rebuilt.',
    })[active?.state] ?? '';
  } catch { /* Keep the current status during a connection interruption. */ }
}

async function loadSession() {
  const id = location.hash.slice(1);
  if (!uuid.test(id) || activeSession?.id === id) return;
  try {
    const response = await fetch(api + '/' + id);
    if (location.hash.slice(1) !== id) return;
    if (response.status === 404) loadRecent();
    if (!response.ok) throw new Error('This session could not be opened.');
    const session = await response.json();
    if (location.hash.slice(1) === id) showSession(session);
  } catch (error) {
    status.textContent = error.message;
  }
}

function choose(selection) {
  if (busy) return;
  files = Array.from(selection);
  const list = element('selected');
  list.replaceChildren(...files.map(file => write('li', file.name)));
  list.hidden = files.length === 0;
  const size = files.reduce((sum, file) => sum + file.size, 0);
  const invalid = files.length > 32 || size > 1024 ** 3 || files.some(file => !uploadName(file));
  status.textContent = invalid ? 'Choose up to 32 MOV or MP4 clips totaling at most 1 GiB.' : '';
  submit.disabled = invalid || files.length === 0;
}

function uploadName(file) {
  if (/\.(mov|mp4)$/i.test(file.name)) return file.name;
  const extensions = {'video/quicktime': '.mov', 'video/mp4': '.mp4', 'video/x-m4v': '.mp4'};
  return extensions[file.type] ? file.name + extensions[file.type] : '';
}

input.addEventListener('change', () => choose(input.files));
drop.addEventListener('dragover', event => {
  event.preventDefault();
  if (!busy) drop.classList.add('over');
});
drop.addEventListener('dragleave', () => drop.classList.remove('over'));
drop.addEventListener('drop', event => {
  event.preventDefault();
  drop.classList.remove('over');
  if (!busy) {
    input.value = '';
    choose(event.dataTransfer.files);
  }
});

function upload(body) {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('POST', api);
    request.responseType = 'json';
    request.upload.onprogress = event => {
      if (event.lengthComputable) progress.value = Math.floor(event.loaded / event.total * 100);
    };
    request.upload.onload = () => {
      progress.removeAttribute('value');
      status.textContent = 'Reading clips and generating previews…';
    };
    request.onload = () => {
      if (request.response) {
        resolve(request.response);
      } else if (request.status === 413) {
        reject(new Error('This batch is too large. Try a smaller batch.'));
      } else {
        reject(new Error('The Pi could not accept this upload. Try again.'));
      }
    };
    request.onerror = () => reject(new Error('Connection to the Pi was interrupted. Keep this page open and try again.'));
    request.send(body);
  });
}

element('import').addEventListener('submit', async event => {
  event.preventDefault();
  if (submit.disabled || busy) return;
  const body = new FormData();
  for (const file of files) body.append('clips', file, uploadName(file));
  const failures = element('failures');
  failures.hidden = true;
  progress.value = 0;
  progress.hidden = false;
  status.textContent = 'Uploading clips…';
  busy = true;
  submit.disabled = input.disabled = true;
  try {
    const result = await upload(body);
    if (result.error) throw new Error(result.error);
    if (result.session) {
      showSession(result.session);
      loadRecent();
      location.hash = result.session.id;
      input.value = '';
      files = [];
      element('selected').hidden = true;
    }
    const message = result.session ? 'Session saved.' : 'No clips were imported.';
    failures.replaceChildren(...(result.failures ?? []).map(failure => write('li', failure)));
    failures.hidden = !failures.children.length;
    status.textContent = message;
  } catch (error) {
    status.textContent = error.message;
  } finally {
    progress.hidden = true;
    busy = false;
    input.disabled = false;
    submit.disabled = files.length === 0;
  }
});

element('delete').addEventListener('click', async () => {
  const session = activeSession;
  if (!session || busy || !window.confirm('Delete this session and its uploaded clips from the Pi?')) return;
  try {
    const response = await fetch(api + '/' + session.id, {method: 'DELETE'});
    if (!response.ok) throw new Error('Could not delete the session.');
    loadRecent();
    if (activeSession === session) {
      activeSession = undefined;
      clearTimeout(trackingPoll);
      resetTracking();
      element('session').hidden = true;
      history.replaceState(null, '', location.pathname);
    }
    status.textContent = 'Session deleted.';
  } catch (error) {
    status.textContent = error.message;
  }
});

loadRecent();
window.addEventListener('focus', loadRecent);
window.addEventListener('focus', () => refreshCoaching(activeSession));
window.addEventListener('skater-selection-saved', () => refreshCoaching(activeSession));
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) loadRecent();
});
window.addEventListener('hashchange', loadSession);
loadSession();
