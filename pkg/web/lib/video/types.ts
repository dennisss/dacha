// NOTE: All times are in floating point seconds units.

export type VideoSourceOptions = LiveVideoSourceOptions | FragmentedVideoSourceOptions;

export enum VideoSourceKind {
    Live,
    Fragmented
}

// This source plays back a live stream via a single HTTP endpoint which when queried gives us an
// infinite MP4 which starts at the current point in time.
//
// The current time will always be displayed as the duration since the most recent HTTP stream
// to the server started.
//
// Note that this source does not support seeking and is always either:
// 1. seeking ('buffering') to now.
// 2. playing at now.
// 3. paused
export interface LiveVideoSourceOptions {
    kind: VideoSourceKind.Live;
    url: string;
}

// This source plays back a fixed length video which has been split up into 
export interface FragmentedVideoSourceOptions {
    kind: VideoSourceKind.Fragmented;

    // Earliest seekable time in the video timeline.
    // Defaults to the smallest start_time in all the fragments.
    // (also works as the initial time at which the video player will start playback).
    start_time?: number;

    // Defaults to the largest end_time in all the fragments.
    end_time?: number;

    // NOTE: These MUST be sorted by start time and MUST NOT be empty. The player will throw an error if this is empty.
    fragments: MediaFragment[];
}

// Meant to match the cnc.MediaFragment protobuf.
export interface MediaFragment {
    start_time: number;
    end_time: number;
    init_data?: MediaSegmentData;
    data: MediaSegmentData;
    relative_start: number;
    mime_type: string;
}

// Meant to match the cnc.MediaSegmentData protobuf.
export interface MediaSegmentData {
    segment_url: string

    // TODO: Merge HTTP requests for adjacent byte ranges on the client side.
    byte_range?: ByteRange
}

export interface ByteRange {
    start: number
    end: number
}

export interface VideoState {
    // If true, the video isn't advancing and won't advance automatically once seeking is done.
    paused: boolean;

    // If true, the video is stopped waiting for more data to be available.
    seeking: boolean;

    // If true, the video is currently in an error back off state (e.g. some network requests
    // failed).
    error: boolean;

    // Current time in seconds of the video.
    current_time: number;

    // If the video is seekable, this is the overall time range over which the video can be seeked.
    timeline?: Range;
}

export interface Range {
    start: number;
    end: number;
}

export type VideoStateChangeHandler = (state: VideoState) => void;

// NOTE: One VideoSource instance exists for the lifetime of a VideoPlayer. New VideoSource instances will be created by the SourceKind changes though. 
export abstract class VideoSource {
    // Updates the video source to operate using a new set of options (with the same kind).
    abstract update(options: VideoSourceOptions): void;

    // Requests that the video start playing.
    abstract play(): void;

    // Requests that the video stop playing.
    abstract pause(): void;

    // 
    abstract seek(time: number): void;
}
