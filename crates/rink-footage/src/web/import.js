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

const dialog = element('import-dialog');
element('new-session').addEventListener('click', () => dialog.showModal());
element('close-import').addEventListener('click', () => dialog.close());
dialog.addEventListener('cancel', event => { if (busy) event.preventDefault(); });

let sessions = [];
function showLibrary() {
  const query = element('search').value.trim().toLocaleLowerCase();
  const matches = sessions.filter(session => session.name.toLocaleLowerCase().includes(query));
  element('library-summary').textContent = sessions.length + (sessions.length === 1 ? ' saved session' : ' saved sessions');
  element('empty').hidden = matches.length > 0;
  element('empty').textContent = sessions.length ? 'No sessions match this name.' : 'Import game footage to start your first session.';
  element('recent').replaceChildren(...matches.map(session => {
    const row = document.createElement('li');
    const link = document.createElement('a');
    link.href = '?session=' + session.id;
    const count = session.clip_count + (session.clip_count === 1 ? ' clip' : ' clips');
    const preview = clip => api + '/' + session.id + '/clips/' + clip + '/preview';
    const [first,...rest] = session.preview_clips;
    const sheet = thumbnail(preview(first), count);
    sheet.classList.add('sheet-' + session.preview_clips.length);
    for (const clip of rest) {
      const still = document.createElement('img');
      still.src = preview(clip);
      still.alt = '';
      still.loading = 'lazy';
      sheet.append(still);
    }
    const seconds = Math.floor(session.duration_ms / 1000);
    const duration = Math.floor(seconds / 60) + ':' + String(seconds % 60).padStart(2,'0');
    link.append(sheet, cardInfo(session.name, date(session.created_ms) + ' · ' + duration + ' total'));
    row.append(link);
    return row;
  }));
}
function thumbnail(src, badge) {
  const thumb = document.createElement('div');
  thumb.className = 'thumb';
  const image = document.createElement('img');
  image.src = src;
  image.alt = '';
  image.loading = 'lazy';
  thumb.append(image, write('small', badge));
  return thumb;
}
function cardInfo(title, detail) {
  const info = document.createElement('div');
  info.className = 'card-info';
  info.append(write('strong',title),write('span',detail));
  return info;
}
element('search').addEventListener('input', showLibrary);

async function loadRecent() {
  const request = ++recentRequest;
  try {
    const response = await fetch(api);
    if (!response.ok) throw new Error();
    const recent = await response.json();
    if (request !== recentRequest) return;
    sessions = recent;
    showLibrary();
  } catch {
    if (request !== recentRequest) return;
    element('empty').hidden = false;
    element('empty').textContent = 'The session library could not be loaded. Refresh to retry.';
  }
}

function showSession(session) {
  clearTimeout(trackingPoll);
  resetTracking();
  activeSession = session;
  element('session').hidden = false;
  element('library').hidden = true;
  element('session-title').textContent = new Intl.DateTimeFormat(undefined, {month:'long',day:'numeric'}).format(session.created_ms) + ' session';
  element('session-detail').textContent =
    session.clips.length + (session.clips.length === 1 ? ' clip · ' : ' clips · ') + date(session.created_ms);
  const source = element('youtube-source');
  source.hidden = !session.source_url;
  if (session.source_url) source.href = session.source_url;
  element('clips').replaceChildren(...session.clips.map(clip => {
    const card = document.createElement('article');
    card.className = 'clip';
    const seconds = Math.floor(clip.duration_ms / 1000);
    const duration = Math.floor(seconds / 60) + ':' + String(seconds % 60).padStart(2, '0');
    const info = cardInfo(clip.name, 'Checking tracking status…');
    info.lastChild.id = 'track-status-' + clip.id;
    const review = write('button', 'Review players');
    review.type = 'button';
    review.hidden = true;
    review.id = 'track-review-' + clip.id;
    review.addEventListener('click', () => openTracking(session, clip));
    const coaching = write('a', 'Coaching Film Room');
    coaching.className = 'clip-coaching';
    coaching.id = 'coaching-' + clip.id;
    coaching.hidden = true;
    info.append(review,coaching);
    card.dataset.clip = clip.id;
    card.append(thumbnail(api + '/' + session.id + '/clips/' + clip.id + '/preview',duration),info);
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
    if (response.status === 404) {
      if (activeSession === session) for (const clip of session.clips) element('track-status-' + clip.id).textContent = labels.disabled;
      return;
    }
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

function openTracking(session, clip, moment) {
  activeClip = clip.id;
  element('coaching-step').hidden = true;
  for (const card of element('clips').children) card.classList.toggle('selected', card.dataset.clip === clip.id);
  const video = element('tracking-video');
  const base = api + '/' + session.id + '/clips/' + clip.id + '/tracks';
  video.src = base + '.mp4';
  element('tracking-data').href = base + '.ndjson';
  element('tracking-title').textContent = clip.name;
  element('tracking-review').hidden = false;
  window.skaterReview.open(session, clip, base, moment);
  refreshCoaching(session);
  element('tracking-review').scrollIntoView({block:'start'});
  element('tracking-viewport').focus({preventScroll:true});
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
      identity_changed: 'Player study found different identities in Your skater selection. Correct the selected track at the switch.',
    })[active?.state] ?? '';
  } catch { /* Keep the current status during a connection interruption. */ }
}

async function loadSession() {
  const selectedId = () => new URLSearchParams(location.search).get('session') ?? location.hash.slice(1);
  const id = selectedId();
  if (!uuid.test(id) || activeSession?.id === id) return;
  try {
    const response = await fetch(api + '/' + id);
    if (selectedId() !== id) return;
    if (response.status === 404) loadRecent();
    if (!response.ok) throw new Error('This session could not be opened.');
    const session = await response.json();
    if (selectedId() === id) showSession(session);
  } catch (error) {
    element('page-status').textContent = error.message;
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
  element('close-import').disabled = true;
  submit.disabled = input.disabled = true;
  try {
    const result = await upload(body);
    if (result.error) throw new Error(result.error);
    if (result.session) {
      showSession(result.session);
      loadRecent();
      history.pushState(null,'','?session=' + result.session.id);
      input.value = '';
      files = [];
      element('selected').hidden = true;
    }
    const message = result.session ? 'Session saved.' : 'No clips were imported.';
    failures.replaceChildren(...(result.failures ?? []).map(failure => write('li', failure)));
    failures.hidden = !failures.children.length;
    status.textContent = message;
    if (result.session && failures.hidden) {
      dialog.close();
      element('page-status').textContent = message;
    }
  } catch (error) {
    status.textContent = error.message;
  } finally {
    progress.hidden = true;
    busy = false;
    element('close-import').disabled = false;
    input.disabled = false;
    submit.disabled = files.length === 0;
  }
});

const youtubeForm = element('youtube-import');
const youtubeSubmit = element('youtube-submit');
const youtubeProgress = element('youtube-progress');
let youtubeRequest;
try { youtubeRequest = localStorage.getItem('rink.youtube-import'); } catch {}

function pendingYoutube(id) {
  youtubeRequest = id;
  youtubeSubmit.disabled = Boolean(id);
  try {
    if (id) localStorage.setItem('rink.youtube-import', id);
    else localStorage.removeItem('rink.youtube-import');
  } catch {}
}

async function youtubeJson(path, options) {
  const response = await fetch('/footage/api/youtube' + path, options);
  const result = await response.json().catch(() => ({}));
  if (!response.ok) {
    const error = new Error(result.error ?? 'Could not connect to the video importer.');
    error.status = response.status;
    throw error;
  }
  return result;
}

async function watchYoutube(id) {
  if (youtubeRequest !== id) return;
  try {
    const job = await youtubeJson('/' + id);
    if (youtubeRequest !== id) return;
    if (job.state === 'ready') {
      pendingYoutube(null);
      loadRecent();
      youtubeProgress.replaceChildren('YouTube video saved. ', write('a', 'Open session'));
      youtubeProgress.querySelector('a').href = '?session=' + id;
      return;
    }
    if (job.state === 'failed') {
      pendingYoutube(null);
      youtubeProgress.textContent = job.error;
      return;
    }
    youtubeProgress.textContent = job.state === 'fetching' ? 'Fetching the YouTube video…' : 'YouTube video queued…';
  } catch (error) {
    if (error.status === 404) {
      pendingYoutube(null);
      youtubeProgress.textContent = error.message;
      return;
    }
    youtubeProgress.textContent = 'Checking the YouTube import…';
  }
  setTimeout(() => watchYoutube(id), 4000);
}

if (youtubeRequest) {
  pendingYoutube(youtubeRequest);
  watchYoutube(youtubeRequest);
}
youtubeForm.addEventListener('submit', async event => {
  event.preventDefault();
  if (youtubeSubmit.disabled || busy) return;
  const field = element('youtube-link');
  const feedback = element('youtube-status');
  feedback.textContent = '';
  youtubeSubmit.disabled = true;
  try {
    const job = await youtubeJson('', {
      method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({url:field.value}),
    });
    pendingYoutube(job.id);
    field.value = '';
    dialog.close();
    await watchYoutube(job.id);
  } catch (error) {
    feedback.textContent = error.message;
    youtubeSubmit.disabled = false;
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
    element('library').hidden = false;
    element('page-status').textContent = 'Session deleted.';
  } catch (error) {
    element('page-status').textContent = error.message;
  }
});

loadRecent();
window.addEventListener('focus', loadRecent);
window.addEventListener('focus', () => refreshCoaching(activeSession));
window.addEventListener('skater-selection-saved', () => refreshCoaching(activeSession));
window.addEventListener('player-identity-saved', () => refreshCoaching(activeSession));
window.addEventListener('study-moment-open', event => {
  const clip = activeSession?.clips.find(clip => clip.id === event.detail.view.clip);
  if (clip) openTracking(activeSession,clip,event.detail);
});
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) loadRecent();
});
window.addEventListener('popstate', () => {
  if (location.hash || location.search) { loadSession(); return; }
  clearTimeout(trackingPoll);
  activeSession = undefined;
  resetTracking();
  element('session').hidden = true;
  element('library').hidden = false;
  loadRecent();
});
window.addEventListener('hashchange', loadSession);
loadSession();
