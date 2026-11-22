import * as api from "./api";
import type {
  Anchor,
  Candidate,
  Clip,
  Frame,
  FrameKey,
  Identity,
  IdentityAction,
  Moment,
  SavedPlay,
  Selections,
  Session,
  Track,
  View,
} from "./api";
import { cuts, gearMatch, indexAtMs, jerseyLabel, runs, summarize, trackLabel, type Run } from "./tracks";

/** What the reviewer is looking at: a raw track id, or a confirmed player's id. */
export type Selection = number | string | null;
/** A second skater to compare with. "you" is the saved "this is me" track. */
export type Partner = number | string | "you" | null;

/** A play being created or edited. `id` is null until it is first saved. */
export type Draft = {
  id: string | null;
  start_ms: number;
  decision_ms: number;
  end_ms: number;
  primary: string;
  secondary: string;
  cue: string;
  attach: boolean;
};

export type PlayerFields = { name: string; team: string; jersey: string };
/** A note on an unconfirmed track. It stays in this browser; notes on confirmed players go to the Pi. */
export type LocalNote = { key: string; player: number; partner: Partner; index: number; text: string };
export type Offers = { frame: FrameKey; clear: string | null; candidates: Candidate[] };

const NOTE_PREFIX = "rink.study-note.";
const SPEEDS = [0.25, 0.5, 1, 2];

const sameFrame = (a: FrameKey | null | undefined, b: FrameKey | null | undefined) =>
  !!a && !!b && a.clip === b.clip && a.run === b.run && a.source_ms === b.source_ms && a.track_id === b.track_id;

/** One clip open for review: its tracked frames, the playhead, the selection and every edit. */
export class Review {
  readonly session: Session;
  readonly clip: Clip;
  readonly frames: Frame[];
  readonly run: string | null;
  /** Source time of the first frame; the preview video starts there. */
  readonly first: number;
  readonly span: number;
  readonly summary;
  readonly cuts: number[];

  identity = $state.raw<Identity>(undefined!);
  selections = $state.raw<Selections>({});
  index = $state(0);
  playing = $state(false);
  rate = $state(1);
  selected = $state<Selection>(null);
  partner = $state<Partner>(null);
  draft = $state<Draft | null>(null);
  offers = $state.raw<Offers | null>(null);
  saving = $state(false);
  status = $state("");
  localNotes = $state.raw<LocalNote[]>([]);

  /** Called after a saved edit, so the workspace can refresh what depends on it. */
  onsaved = () => {};
  /** Called to jump to a moment saved in another clip of the session. */
  onopen = (_clip: string, _moment: Moment) => {};

  /**
   * The source time at the video's zero. The original clip starts at 0; a tracking preview starts
   * at the clip's first tracked frame.
   */
  origin = 0;
  #video: HTMLVideoElement | undefined;
  #stopBefore: number | null = null;
  #suggesting = 0;
  #edits = 0;

  static async open(session: Session, clip: Clip): Promise<Review> {
    const [data, selections, identity] = await Promise.all([
      api.tracks(session.id, clip.id),
      api.selections(session.id),
      api.identity(session.id),
    ]);
    return new Review(session, clip, data.frames, data.run, identity, selections);
  }

  constructor(
    session: Session,
    clip: Clip,
    frames: Frame[],
    run: string | null,
    identity: Identity,
    selections: Selections,
  ) {
    this.session = session;
    this.clip = clip;
    this.frames = frames;
    this.run = run;
    this.first = frames[0].source_ms;
    this.span = Math.max(1, frames.at(-1)!.source_ms - this.first);
    this.summary = summarize(frames);
    this.cuts = cuts(frames);
    this.identity = identity;
    this.selections = selections;
    this.localNotes = this.#readLocalNotes();
  }

  frame = $derived.by(() => this.frames[this.index]);
  /** Identity edits only apply to the tracking run they were made on. */
  editable = $derived.by(() => this.run != null && this.identity.runs[this.clip.id] === this.run);

  /** This clip's assignments per track, oldest first. A track is that player from the assignment on. */
  #assignments = $derived.by(() => {
    const map = new Map<number, { source_ms: number; player: string }[]>();
    for (const { view, player } of this.identity.assignments) {
      if (view.clip !== this.clip.id || view.run !== this.run) continue;
      const list = map.get(view.track_id) ?? [];
      map.set(view.track_id, list);
      list.push({ source_ms: view.source_ms, player });
    }
    for (const list of map.values()) list.sort((a, b) => a.source_ms - b.source_ms);
    return map;
  });

  anchors = $derived.by<Anchor[]>(() => this.selections[this.clip.id] ?? []);
  /** The "this is me" track on the current frame, if it is in view. */
  me = $derived.by(() => {
    const anchor = this.#anchorAt(this.frame.source_ms);
    return { anchor, track: anchor && this.frame.tracks.find((track) => track.id === anchor.track_id) };
  });

  selectedTrack = $derived(this.resolve(this.frame, this.selected, true));
  /** The selected skater on this frame, when it can be edited. */
  target = $derived<FrameKey | null>(
    this.selectedTrack && this.editable ? this.#key(this.frame, this.selectedTrack) : null,
  );
  targetPlayer = $derived(this.selectedTrack ? this.playerOn(this.frame.source_ms, this.selectedTrack.id) : null);
  isMe = $derived(!!this.selectedTrack && this.me.track?.id === this.selectedTrack.id);

  /** Where the selection is in view, and with a partner, where both are. */
  appearances = $derived.by<Run[]>(() => {
    if (this.selected == null) return [];
    return runs(this.frames, (frame) => {
      const watched = this.resolve(frame, this.selected, true);
      if (!watched) return false;
      if (this.partner == null) return true;
      const other = this.companion(frame, true);
      return !!other && other.id !== watched.id;
    });
  });

  myAppearances = $derived.by<Run[]>(() => {
    if (!this.anchors.length) return [];
    return runs(this.frames, (frame) => {
      const id = this.#anchorAt(frame.source_ms)?.track_id;
      return frame.tracks.some((track) => track.id === id && !track.predicted);
    });
  });

  plays = $derived(this.identity.plays.filter((play) => play.clip === this.clip.id));
  moments = $derived(this.identity.moments);

  /** Confirmed players across the session, then this clip's tracks nobody has named. */
  roster = $derived.by(() => {
    const here = new Set<string>();
    for (const list of this.#assignments.values()) for (const { player } of list) here.add(player);
    const players = Object.entries(this.identity.players)
      .map(([id, name]) => ({ id, name, here: here.has(id) }))
      .sort((a, b) => Number(b.here) - Number(a.here) || a.name.localeCompare(b.name));
    const tracks = [...this.summary.entries()]
      .filter(([id]) => !this.#assignments.has(id))
      .map(([id, summary]) => ({ id, ...summary }))
      .sort((a, b) => a.id - b.id);
    return { players, tracks };
  });

  #key(frame: Frame, track: Track): FrameKey {
    return { clip: this.clip.id, run: this.run!, source_ms: frame.source_ms, track_id: track.id };
  }

  #anchorAt(ms: number): Anchor | undefined {
    return this.anchors.findLast((anchor) => anchor.source_ms <= ms) ?? this.anchors[0];
  }

  playerOn(ms: number, track: number): string | null {
    return this.#assignments.get(track)?.findLast((entry) => entry.source_ms <= ms)?.player ?? null;
  }

  /** The track a selection is on in a frame. `detected` skips boxes the tracker only predicted. */
  resolve(frame: Frame, selection: Selection, detected = false): Track | undefined {
    if (selection == null) return undefined;
    return frame.tracks.find(
      (track) =>
        (!detected || !track.predicted) &&
        (typeof selection === "number"
          ? track.id === selection
          : this.playerOn(frame.source_ms, track.id) === selection),
    );
  }

  companion(frame: Frame, detected = false): Track | undefined {
    if (this.partner !== "you") return this.resolve(frame, this.partner, detected);
    return this.resolve(frame, this.#anchorAt(frame.source_ms)?.track_id ?? null, detected);
  }

  name(selection: Selection | "you"): string {
    if (selection == null) return "";
    if (selection === "you") return "You";
    if (typeof selection === "string") return this.identity.players[selection] ?? "Unknown player";
    const jersey = this.summary.get(selection)?.jersey;
    return jersey ? `${trackLabel(selection)} · ${jersey}` : trackLabel(selection);
  }

  /** The label drawn on a track's box. */
  tag(frame: Frame, track: Track): string {
    const player = this.playerOn(frame.source_ms, track.id);
    return (player && this.identity.players[player]) || trackLabel(track.id);
  }

  indexAt(ms: number): number {
    return indexAtMs(this.frames, ms);
  }

  // Playback. The video element is the clock; `index` follows it.

  attach(video: HTMLVideoElement | undefined, origin = this.origin) {
    this.#video = video;
    this.origin = origin;
    if (video) video.playbackRate = this.rate;
  }

  /** The video presented the frame at `seconds`. */
  tick(seconds: number) {
    const ms = this.origin + Math.round(seconds * 1000);
    const index = indexAtMs(this.frames, ms);
    if (this.#stopBefore != null) {
      const pastEnd = this.#stopBefore === this.frames.length && ms > this.first + this.span + 50;
      if (index >= this.#stopBefore || pastEnd) {
        const end = this.#stopBefore - 1;
        this.#stopBefore = null;
        this.#video?.pause();
        this.#seek(end);
        return;
      }
    }
    this.index = index;
  }

  #seek(index: number) {
    this.index = Math.max(0, Math.min(this.frames.length - 1, index));
    // A hair past the frame's time, so the decoder lands on it and not the one before.
    if (this.#video) this.#video.currentTime = Math.max(0, (this.frame.source_ms - this.origin) / 1000 + 0.01);
  }

  /** Put a newly loaded video on the current frame, without touching playback. */
  resync() {
    this.#seek(this.index);
  }

  /** Pause on a frame. */
  seek(index: number) {
    this.#stopBefore = null;
    this.#video?.pause();
    this.#seek(index);
  }

  seekMs(ms: number) {
    this.seek(this.indexAt(ms));
  }

  toggle() {
    if (!this.#video) return;
    if (this.#video.paused) {
      this.#video.play().catch(() => (this.status = "The clip could not play."));
    } else {
      this.#video.pause();
    }
  }

  /** Play frames `start` to `end` and stop there. */
  playRange(start: number, end: number) {
    this.seek(start);
    this.#stopBefore = end + 1;
    this.#video?.play().catch(() => (this.#stopBefore = null));
  }

  /** Called by the stage when the video starts or stops. */
  setPlaying(playing: boolean) {
    this.playing = playing;
    if (playing) {
      this.offers = null;
      this.status = "";
    }
  }

  cycleRate() {
    this.rate = SPEEDS[(SPEEDS.indexOf(this.rate) + 1) % SPEEDS.length];
    if (this.#video) this.#video.playbackRate = this.rate;
  }

  jumpCut(direction: 1 | -1) {
    const next =
      direction > 0 ? this.cuts.find((cut) => cut > this.index) : this.cuts.findLast((cut) => cut < this.index);
    if (next != null) this.seek(next);
  }

  // Selection.

  select(selection: Selection) {
    if (selection === this.selected) return;
    this.selected = selection;
    if (selection == null || this.partner === selection) this.partner = null;
    this.offers = null;
    this.status = "";
  }

  /** Select whoever a box belongs to: its confirmed player, or the bare track. */
  selectTrack(track: Track, asPartner = false) {
    const selection = this.playerOn(this.frame.source_ms, track.id) ?? track.id;
    if (asPartner && this.selected != null && selection !== this.selected) this.partner = selection;
    else this.select(selection);
  }

  /** Step through the detected skaters on this frame, left to right. */
  selectNext(direction: 1 | -1) {
    const tracks = this.frame.tracks.filter((track) => !track.predicted).sort((a, b) => a.xyxy[0] - b.xyxy[0]);
    if (!tracks.length) return;
    const at = tracks.findIndex((track) => track.id === this.selectedTrack?.id);
    this.selectTrack(tracks[(at + direction + tracks.length) % tracks.length]);
  }

  // Identity. Every edit is appended to the session's ledger on the Pi, which answers with the new state.

  async #save(action: IdentityAction): Promise<boolean> {
    if (this.saving) return false;
    this.saving = true;
    const edit = { ...action, id: crypto.randomUUID(), revision: this.identity.revision };
    try {
      // The edit id makes one retry after a dropped connection safe.
      const state = await api.editIdentity(this.session.id, edit).catch((error) => {
        if (error instanceof TypeError) return api.editIdentity(this.session.id, edit);
        throw error;
      });
      const target = this.target;
      this.#apply(state);
      if (action.action === "assign") {
        this.selected = action.player;
        if (this.partner === action.player) this.partner = null;
      } else if (action.action === "undo" && target) {
        this.selected = this.playerOn(target.source_ms, target.track_id) ?? target.track_id;
      }
      this.status = action.action === "undo" ? "Edit undone." : "Saved.";
      this.onsaved();
      return true;
    } catch (error) {
      const latest = await api.identity(this.session.id).catch(() => null);
      if (latest) this.#apply(latest);
      this.status = (error as Error).message;
      return false;
    } finally {
      this.saving = false;
    }
  }

  #apply(state: Identity) {
    this.identity = state;
    this.offers = null;
  }

  /** Checks the name fields, falling back to "Team #12" when only a jersey is given. */
  #fields(fields: PlayerFields, player: string | null): { name: string; details: api.PlayerDetails } | null {
    const details = { team: fields.team.trim(), jersey: fields.jersey.trim() };
    if (!/^[0-9]{0,2}$/.test(details.jersey)) {
      this.status = "Use one or two digits for the jersey number, or leave it blank.";
      return null;
    }
    const typed = fields.name.trim();
    const previous = player ? jerseyLabel(this.identity.player_details[player]) : "";
    const name = typed && typed !== previous ? typed : jerseyLabel(details);
    if (!name) {
      this.status = "Enter a name or a jersey number.";
      return null;
    }
    return { name, details };
  }

  #shown(): string[] {
    return this.offers?.candidates.map((candidate) => candidate.player) ?? [];
  }

  /** Name the selected skater as a new player, from this frame on. */
  async confirmNew(fields: PlayerFields): Promise<boolean> {
    const checked = this.target && this.#fields(fields, null);
    if (!checked) return false;
    return this.#save({
      action: "assign",
      frame: this.target!,
      player: crypto.randomUUID(),
      ...checked,
      shown: this.#shown(),
    });
  }

  /** The selected skater is this already-confirmed player, from this frame on. */
  async link(player: string): Promise<boolean> {
    if (!this.target) return false;
    return this.#save({ action: "assign", frame: this.target, player, name: null, shown: this.#shown() });
  }

  async rename(fields: PlayerFields): Promise<boolean> {
    const player = this.targetPlayer;
    const checked = this.target && player && this.#fields(fields, player);
    if (!checked) return false;
    return this.#save({ action: "rename", frame: this.target!, player: player!, ...checked });
  }

  /** A suggested player is not this skater; ask for the next best. */
  async reject(player: string) {
    const frame = this.target;
    if (!frame || !this.#shown().includes(player)) return;
    if (await this.#save({ action: "reject", frame, player, shown: this.#shown() })) await this.suggest();
  }

  async undo() {
    if (this.identity.undo) await this.#save({ action: "undo", target: this.identity.undo });
  }

  /** Ask the Pi which confirmed players look like the selected skater on this frame. */
  async suggest() {
    const frame = this.target;
    if (!frame) return;
    const token = ++this.#suggesting;
    try {
      const result = await api.suggestions(this.session.id, frame);
      if (token !== this.#suggesting || !sameFrame(frame, this.target)) return;
      if (result.revision !== this.identity.revision) {
        this.#apply(await api.identity(this.session.id));
        return;
      }
      this.offers = { ...result, frame };
    } catch (error) {
      if (token === this.#suggesting) this.status = (error as Error).message;
    }
  }

  /** A confirmed frame of a player to recognise them by: the suggestion's, or their latest elsewhere. */
  reference(player: string): View | undefined {
    const offered = this.offers?.candidates.find((candidate) => candidate.player === player)?.reference;
    if (offered) return offered;
    return this.identity.assignments.findLast(
      (assignment) => assignment.player === player && !sameFrame(assignment.view, this.target),
    )?.view;
  }

  // "This is me". Anchors say which track the user is from a frame on; a later anchor corrects an ID switch.

  async setMe(on: boolean) {
    const track = this.selectedTrack;
    const anchor = this.me.anchor;
    if (this.saving || (on ? !track : !anchor)) return;
    this.saving = true;
    this.#edits++;
    try {
      const saved = on
        ? await api.setAnchor(this.session.id, this.clip.id, this.frame.source_ms, track!.id)
        : await api.setAnchor(this.session.id, this.clip.id, anchor!.source_ms, null);
      this.selections = { ...this.selections, [this.clip.id]: saved };
      if (!saved.length && this.partner === "you") this.partner = null;
      this.status = "Saved.";
      this.onsaved();
    } catch (error) {
      this.status = (error as Error).message;
    } finally {
      this.saving = false;
    }
  }

  /** True when another clip has a "this is me" with gear colors to match against. */
  canFindMe = $derived(
    !this.anchors.length &&
      Object.entries(this.selections).some(
        ([clip, anchors]) => clip !== this.clip.id && anchors.some((anchor) => anchor.gear),
      ),
  );

  /** Select the track whose gear matches the user's in another clip. They still confirm it. */
  findMe() {
    const seed = Object.entries(this.selections)
      .filter(([clip]) => clip !== this.clip.id)
      .flatMap(([, anchors]) => anchors)
      .find((anchor) => anchor.gear);
    const match = seed?.gear && gearMatch(this.frames, seed.gear);
    if (!match) {
      this.status = "No clear gear match. Select yourself in the video.";
      return;
    }
    this.seekMs(match.source_ms);
    this.select(this.playerOn(match.source_ms, match.id) ?? match.id);
    this.status = "Is this you? Check the box to confirm.";
  }

  // Plays: a labelled stretch of the clip around a decision frame.

  #ms(index: number): number {
    return this.frames[Math.max(0, Math.min(this.frames.length - 1, index))].source_ms;
  }

  #snap(ms: number): number {
    return this.#ms(this.indexAt(ms));
  }

  /** Start a play around the playhead, or from an explicit range. */
  startPlay(start_ms?: number, end_ms?: number) {
    const now = this.frame.source_ms;
    const start = this.#snap(start_ms ?? now - 2500);
    const end = this.#snap(end_ms ?? now + 1500);
    this.#video?.pause();
    this.draft = {
      id: null,
      start_ms: start,
      end_ms: end,
      decision_ms: Math.max(start, Math.min(end, now)),
      primary: "",
      secondary: "",
      cue: "",
      attach: this.selected != null,
    };
    this.status = "";
  }

  editPlay(play: SavedPlay) {
    if (play.player) this.select(play.player.player ?? play.track);
    this.draft = {
      id: play.id,
      start_ms: play.start_ms,
      end_ms: play.end_ms,
      decision_ms: play.decision_ms,
      primary: play.primary,
      secondary: play.secondary ?? "",
      cue: play.cue,
      attach: play.track != null,
    };
    this.seekMs(play.decision_ms);
  }

  /** Set the draft's range from a drag across the timeline, starting a play if there is none. */
  range(a_ms: number, b_ms: number) {
    const start = this.#snap(Math.min(a_ms, b_ms));
    const end = Math.max(this.#snap(Math.max(a_ms, b_ms)), this.#ms(this.indexAt(start) + 1));
    if (!this.draft) this.startPlay(start, end);
    const draft = this.draft!;
    draft.start_ms = start;
    draft.end_ms = end;
    // The decision is the playhead when it is inside the range, else late in it, as most plays build up first.
    const now = this.frame.source_ms;
    draft.decision_ms = now >= start && now <= end ? now : this.#snap(start + (end - start) * 0.625);
  }

  /** Move one edge of the draft to a frame. The decision frame stays inside the range. */
  trim(edge: "start" | "end" | "decision", ms: number) {
    const draft = this.draft;
    if (!draft) return;
    const at = this.#snap(ms);
    if (edge === "start") draft.start_ms = Math.min(at, this.#ms(this.indexAt(draft.end_ms) - 1));
    else if (edge === "end") draft.end_ms = Math.max(at, this.#ms(this.indexAt(draft.start_ms) + 1));
    else draft.decision_ms = at;
    if (edge === "decision") {
      draft.start_ms = Math.min(draft.start_ms, at);
      draft.end_ms = Math.max(draft.end_ms, at);
    } else {
      draft.decision_ms = Math.max(draft.start_ms, Math.min(draft.end_ms, draft.decision_ms));
    }
  }

  /** Mark the in or out point at the playhead, starting a play if there is none. */
  mark(edge: "start" | "end") {
    if (!this.draft) this.startPlay();
    this.trim(edge, this.frame.source_ms);
  }

  async savePlay(): Promise<boolean> {
    const draft = this.draft;
    if (!draft || !this.run) return false;
    if (!draft.primary) {
      this.status = "Choose a label for the play.";
      return false;
    }
    const decision = this.frames[this.indexAt(draft.decision_ms)];
    const track = draft.attach ? this.resolve(decision, this.selected, true) : undefined;
    if (draft.attach && !track) {
      this.status = "The selected skater is not in view at the decision frame. Move it or clear the checkbox.";
      return false;
    }
    const saved = await this.#save({
      action: "save_play",
      play: {
        id: draft.id ?? crypto.randomUUID(),
        clip: this.clip.id,
        run: this.run,
        start_ms: draft.start_ms,
        decision_ms: draft.decision_ms,
        end_ms: draft.end_ms,
        primary: draft.primary,
        secondary: draft.secondary || null,
        cue: draft.cue.trim(),
        track: track?.id ?? null,
      },
    });
    if (saved) this.draft = null;
    return saved;
  }

  async removePlay(id: string) {
    if ((await this.#save({ action: "remove_play", target: id })) && this.draft?.id === id) this.draft = null;
  }

  replay(play: { start_ms: number; end_ms: number }) {
    this.playRange(this.indexAt(play.start_ms), this.indexAt(play.end_ms));
  }

  /** True when a saved play was made on an older tracking run or its skater has since been reassigned. */
  playChanged(play: SavedPlay): boolean {
    return (
      play.run !== this.run ||
      (!!play.player && this.#stateAssignment(play.player.view) !== play.player.player)
    );
  }

  // Notes on what a skater does at a moment.

  async addNote(text: string): Promise<boolean> {
    const note = text.trim();
    const track = this.selectedTrack;
    if (!note || this.selected == null) return false;
    const player = track && this.playerOn(this.frame.source_ms, track.id);
    if (track && player && this.editable) {
      const other = this.companion(this.frame, true);
      const compared = other && this.playerOn(this.frame.source_ms, other.id) !== player ? other : null;
      return this.#save({
        action: "save_moment",
        frame: this.#key(this.frame, track),
        player,
        compare: compared ? this.#key(this.frame, compared) : null,
        text: note,
      });
    }
    if (typeof this.selected === "string") {
      this.status = "Move to a frame where this player is in view to save the note.";
      return false;
    }
    const key = NOTE_PREFIX + this.clip.id + "." + crypto.randomUUID();
    const local = { player: this.selected, partner: this.partner, index: this.index, text: note };
    try {
      localStorage.setItem(key, JSON.stringify(local));
    } catch {
      this.status = "Could not save this note on this device.";
      return false;
    }
    this.localNotes = [...this.localNotes, { ...local, key }].sort((a, b) => a.index - b.index);
    return true;
  }

  removeLocalNote(note: LocalNote) {
    try {
      localStorage.removeItem(note.key);
    } catch {
      this.status = "Could not remove the note.";
      return;
    }
    this.localNotes = this.localNotes.filter((other) => other !== note);
  }

  removeMoment(id: string) {
    return this.#save({ action: "remove_moment", target: id });
  }

  openLocalNote(note: LocalNote) {
    this.select(note.player);
    this.partner = note.partner === "you" && !this.anchors.length ? null : note.partner;
    this.seek(note.index);
  }

  openMoment(moment: Moment) {
    if (moment.view.clip !== this.clip.id) {
      this.onopen(moment.view.clip, moment);
      return;
    }
    this.select(moment.player);
    this.partner = moment.compare?.player ?? moment.compare?.view.track_id ?? null;
    this.seekMs(moment.view.source_ms);
    if (this.momentChanged(moment)) this.status = "The identity assignment at this note has changed.";
  }

  momentChanged(moment: Moment): boolean {
    return (
      this.#stateAssignment(moment.view) !== moment.player ||
      (!!moment.compare && this.#stateAssignment(moment.compare.view) !== moment.compare.player)
    );
  }

  /** Who a saved view is assigned to now, in any clip of the session. */
  #stateAssignment(view: FrameKey): string | null {
    if (this.identity.runs[view.clip] !== view.run) return null;
    let best: api.Assignment | undefined;
    for (const assignment of this.identity.assignments) {
      const other = assignment.view;
      if (
        other.clip === view.clip &&
        other.run === view.run &&
        other.track_id === view.track_id &&
        other.source_ms <= view.source_ms &&
        (!best || other.source_ms >= best.view.source_ms)
      ) {
        best = assignment;
      }
    }
    return best?.player ?? null;
  }

  #readLocalNotes(): LocalNote[] {
    const notes: LocalNote[] = [];
    try {
      const prefix = NOTE_PREFIX + this.clip.id + ".";
      for (const key of Object.keys(localStorage)) {
        if (!key.startsWith(prefix)) continue;
        try {
          const note = JSON.parse(localStorage.getItem(key) ?? "null");
          if (
            this.summary.has(note?.player) &&
            Number.isInteger(note.index) &&
            this.frames[note.index] &&
            typeof note.text === "string"
          ) {
            notes.push({ ...note, key });
          }
        } catch {
          // A note that no longer parses is skipped.
        }
      }
    } catch {
      // Storage can be blocked; then there are no local notes.
    }
    return notes.sort((a, b) => a.index - b.index);
  }

  /** Pick up edits made in another tab or on another device. */
  async refresh() {
    if (this.saving) return;
    const edits = this.#edits;
    try {
      const [selections, identity] = await Promise.all([
        api.selections(this.session.id),
        api.identity(this.session.id),
      ]);
      if (this.saving || edits !== this.#edits) return;
      this.selections = selections;
      if (identity.revision > this.identity.revision) this.#apply(identity);
    } catch {
      // Keep the current state during a connection interruption.
    }
  }
}
