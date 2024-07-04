import { ByteRange, FragmentedVideoSourceOptions, MediaFragment, MediaSegmentData, VideoSource, VideoState, VideoStateChangeHandler } from "./types";

// The largest gap in buffered segments that the browser will automatically skip over without
// pausing. 
const MAX_FRAGMENT_GAP = 0.03;

// Minimum number of seconds ahead of the current playback position that we will keep buffered
// (when playing back fragmented videos).
//
// Note that this can't be too large since we will be in a non-converging battle with the browser's
// LRU cache if this is larger than the max buffer size. 
const FORWARD_BUFFER_TIME: number = 60;

function media_segment_data_to_string(data: MediaSegmentData): string {
    return data.segment_url + (data.byte_range ? (':' + data.byte_range.start + '-' + data.byte_range.end) : '');
}

function get_segment_key(fragment: MediaFragment): string {
    return media_segment_data_to_string(fragment.init_data ? fragment.init_data : fragment.data);
}

interface SegmentEntry {
    init_data: ArrayBuffer | null;

    loaded_ranges: { start_time: number, end_time: number }[];

    tainted: boolean;
}

export class FragmentedVideoSource extends VideoSource {

    _options: FragmentedVideoSourceOptions;

    _video: HTMLVideoElement;

    // Overall abort signal used as a destructor for the source instance.
    _abort_signal: AbortSignal;

    _on_state_change: VideoStateChangeHandler;

    constructor(
        options: FragmentedVideoSourceOptions,
        video: HTMLVideoElement,
        abort_signal: AbortSignal,
        on_state_change: VideoStateChangeHandler
    ) {
        super();

        this._options = options;
        this._video = video;
        this._abort_signal = abort_signal;
        this._on_state_change = on_state_change;

        // TODO: Should do this whenever we get new options.
        this._options.fragments.sort((a, b) => {
            return a.start_time - b.start_time;
        });

        this._run(video);
    }

    play() {
        this._video.play();
    }

    pause() {
        this._video.pause();
    }

    seek(time: number): void {
        this._video.currentTime = time;
    }

    // TODO: Implement update()

    // TODO: Need a lot of error handling and retry backoff in here.
    async _run(video: HTMLVideoElement) {

        let last_state_json = '';

        const media_source = new MediaSource();

        const media_source_opened = new Promise((res, rej) => {
            media_source.addEventListener('sourceopen', () => {
                res(null)
            });
        });

        this._video.src = URL.createObjectURL(media_source);
        await media_source_opened;

        // TODO: Instead create the source buffer on demand so that we can support having zero fragments temporarily.
        // TODO: May throw an error.
        let source_buffer: SourceBuffer = media_source.addSourceBuffer(this._options.fragments[0].mime_type);

        // Need to have at least one fragment.

        let start_time = this._options.start_time || this._options.fragments[0].start_time;
        let end_time = this._options.end_time || this._options.fragments[this._options.fragments.length - 1].end_time;

        // Make the browser aware of the full seekable range.
        // Without this, currentTime can get clipped to the buffered region by the browser on seeks.
        media_source.duration = end_time;
        media_source.setLiveSeekableRange(start_time, end_time);

        video.currentTime = start_time;

        // Source buffers that we currently have added to the media source.
        // We will have one source buffer per distinct MP4/segment file.
        //
        // TODO: More technically, we need to have one per init_data object (maybe per )

        // Segments that are currently (partially) loaded.
        //
        // We define a segment here as a set of fragments that all share the same init data.
        let segments: { [key: string]: SegmentEntry } = {};

        // Key corresponding to the segment for which we have appended init data to the source buffer.
        let last_segment_key = '';

        // TODO; Need to handle seeking across discontinuities.

        while (!this._abort_signal.aborted) {
            if (video.currentTime + MAX_FRAGMENT_GAP < start_time) {
                video.currentTime = start_time;
            }

            if (video.currentTime - MAX_FRAGMENT_GAP > end_time) {
                video.currentTime = end_time;
            }

            let t = video.currentTime;

            // Find a sorted list of all fragments we want to be buffered.
            let wanted_fragments: MediaFragment[] = [];
            let wanted_total_duration = 0;

            // The fragment containing the current time.
            let current_fragment: MediaFragment | null = null;

            for (var i = 0; i < this._options.fragments.length; i++) {
                let fragment = this._options.fragments[i];

                // TODO: Find the first fragment via binary search.

                let is_current = t >= fragment.start_time && t < fragment.end_time;
                let is_after = t < fragment.start_time;

                if (wanted_fragments.length == 0) {
                    if (!(is_current || is_after)) {
                        continue;
                    }
                }

                let wanted_range = { start: fragment.start_time, end: fragment.end_time };
                if (is_current) {
                    wanted_range.start = t;
                    current_fragment = fragment;
                }

                wanted_fragments.push(fragment);

                wanted_total_duration += wanted_range.end - wanted_range.start;
                if (wanted_total_duration >= FORWARD_BUFFER_TIME) {
                    break;
                }
            }

            // Compare the current fragment (the one with the current time in it) with the one
            // immediately after it in play order. If we are about to finish playback of the
            // current fragment and the browser can't seamlessly transition to the next fragment,
            // advance the time to the start of next fragment.
            //
            // - If there is no current fragment, then by default, we are in a fragment that ends
            //   at the current time.
            // - This code enables playback of videos with gaps in the timeline.
            {
                let next_fragment_i = 0;
                let current_fragment_end_time = t;
                if (current_fragment !== null) {
                    next_fragment_i = 1;
                    current_fragment_end_time = current_fragment.end_time;
                }

                if (wanted_fragments.length > next_fragment_i) {
                    let next_fragment_start_time = wanted_fragments[next_fragment_i].start_time;

                    if (t + MAX_FRAGMENT_GAP >= current_fragment_end_time &&
                        next_fragment_start_time - current_fragment_end_time > MAX_FRAGMENT_GAP
                    ) {
                        this._video.currentTime = next_fragment_start_time;
                        t = next_fragment_start_time;
                    }
                }
            }

            // Garbage collect any obsolete segments (since Chrome automatically evicts old data, we can't rely on 'segments' as a source of truth of what is actually buffered).
            // TODO:
            Object.values(segments).map((segment_entry) => {



            })

            /*
            I'd also like to avoid 'segments' getting 

            */

            // TODO: Don't garbage collect the currently playing fragment if the beginning of it was GC'ed but we still need the remainder of the fragment.


            // Remove any fragments that are already buffered
            let unloaded_fragments = wanted_fragments.filter((fragment) => {
                let segment_key = get_segment_key(fragment);

                let segment_entry = segments[segment_key];
                if (segment_entry) {

                    let found = false;
                    segment_entry.loaded_ranges.map((range) => {
                        // TODO: Make this an approximate match.
                        if (range.start_time == fragment.start_time && range.end_time == fragment.end_time) {
                            found = true;
                        }
                    });

                    if (found) {
                        return false;
                    }
                }

                return true;
            });

            // Load one unloaded fragment,
            if (unloaded_fragments.length > 0) {
                let fragment = unloaded_fragments[0];

                // console.log('Loading', fragment);

                let segment_key = get_segment_key(fragment);

                let segment_entry = segments[segment_key];
                if (!segment_entry) {
                    let init_data = null;
                    if (fragment.init_data) {
                        init_data = await this._fetch_url(fragment.init_data.segment_url, fragment.init_data.byte_range, this._abort_signal);
                    }

                    segment_entry = {
                        init_data,
                        loaded_ranges: [],
                        tainted: false
                    };

                    segments[segment_key] = segment_entry;
                }

                // Switch to the right codecs / init data.
                if (last_segment_key != segment_key) {
                    source_buffer.changeType(fragment.mime_type);

                    if (segment_entry.init_data) {
                        await this._append_to_buffer(source_buffer, segment_entry.init_data);
                    }

                    last_segment_key = segment_key;
                }

                // Load the thing.

                // Load the main data for the 
                let data = await this._fetch_url(fragment.data.segment_url, fragment.data.byte_range, this._abort_signal);

                // TODO: We can also do this incrementally if we support calling source_buffer.abort() in the case that we need to retry the RPC.
                source_buffer.timestampOffset = fragment.start_time - fragment.relative_start;
                await this._append_to_buffer(source_buffer, data);

                // console.log('Buffered start', video.buffered.start(0));

                // TODO: ensure that across all segments that there are no overlapping time ranges (or if there are that we have remove the old ones).
                segment_entry.loaded_ranges.push({
                    start_time: fragment.start_time,
                    end_time: fragment.end_time
                });

                // source_buffer.abort()
            }

            let state: VideoState = {
                paused: this._video.paused,
                seeking: this._video.seeking,
                error: false,
                current_time: this._video.currentTime,
                timeline: { start: start_time, end: end_time }
            };

            // TODO: Emit state change events more frequentyl that just once per cycle (ideally just listen to the video player events and always emit an event when they are triggered).
            let state_json = JSON.stringify(state);
            if (state_json != last_state_json) {
                last_state_json = state_json;
                this._on_state_change(state);
            }

            // TODO: This mainly needs to run faster than this in the case of seeking
            await new Promise((res, rej) => {
                setTimeout(() => {
                    res();
                }, 200);
            });
        }

    }

    async _fetch_url(url: string, byte_range: ByteRange | undefined, abort_signal: AbortSignal): Promise<ArrayBuffer> {

        const response = await fetch(url, {
            signal: abort_signal,
            headers: {
                'Range': (byte_range ? `bytes=${byte_range.start}-${byte_range.end - 1}` : '')
            }
        });

        if (!response.ok) {
            if (response.body !== null) {
                response.body.cancel();
            }

            throw new Error('Error returned in GET request: ' + response.status + ': ' + response.statusText);
        }

        let buffer = await response.arrayBuffer();
        // console.log('Fetched ' + buffer.byteLength);

        return buffer;
    }

    async _append_to_buffer(source_buffer: SourceBuffer, data: ArrayBuffer) {
        let update_ended = new Promise((res, rej) => {
            let listener = () => {
                source_buffer.removeEventListener('updateend', listener);
                res(null);
            };
            source_buffer.addEventListener('updateend', listener);
        });

        source_buffer.appendBuffer(data);
        await update_ended;
    }

};