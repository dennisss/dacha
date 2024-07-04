import React from "react";
import { format_timecode_secs } from "../formatting";
import { VideoState } from "./types";


export interface VideoControlsProps {
    state: VideoState;
    isFullscreen: boolean;
    onFullscreenToggle: () => void;
    onPlayToggle: () => void;
    onSeek: (time: number) => void;
}

export class VideoControls extends React.Component<VideoControlsProps> {

    _seek_bar: React.RefObject<HTMLDivElement> = React.createRef();

    constructor(props: VideoControlsProps) {
        super(props);
    }

    _on_seek_click = (e: React.MouseEvent<HTMLDivElement, MouseEvent>) => {
        let seek_rect = this._seek_bar.current.getBoundingClientRect();
        let percentage = (e.clientX - seek_rect.x) / seek_rect.width;

        let timeline = this.props.state.timeline;

        let time = percentage * (timeline.end - timeline.start) + timeline?.start;

        this.props.onSeek(time);
    }

    _render_seek_bar() {
        let state = this.props.state;
        let timeline = state.timeline;
        if (!timeline) {
            return;
        }

        let percent = 100 * (state.current_time - timeline.start) / (timeline.end - timeline.start);

        percent = Math.round(percent * 20) / 20;

        return (
            <div className="video-controls-seek-bar" onClick={this._on_seek_click}>
                <div className="bar">{/* seekable range */}</div>

                <div className="bar" style={{ backgroundColor: '#4af', width: (percent + '%'), }}>
                    {/* completed range */}
                </div>
            </div>
        );
    }

    _render_timecodes() {
        let state = this.props.state;
        let timeline = state.timeline;
        if (!timeline) {
            // TODO: The issue with this case is that it may be incorrect on the first render (before the state has been emitted by the VideoSource).
            let start = state.current_time;
            return (
                <div style={{ display: 'inline-block', fontSize: 12, padding: '15px 10px' }}>
                    {format_timecode_secs(start)}
                </div>
            );
        }

        let start = state.current_time - state.timeline.start;
        let end = state.timeline.end - state.timeline.start;

        return (
            <div style={{ display: 'inline-block', fontSize: 12, padding: '15px 10px' }}>
                {format_timecode_secs(start, end)}
                &nbsp;/&nbsp;
                {format_timecode_secs(end)}
            </div>
        );

    }

    render() {
        let state = this.props.state;
        let paused = state.paused;

        return (
            <div className="video-controls">
                <div className="video-controls-gradient"></div>

                {this._render_seek_bar()}
                <div className="video-controls-button-bar" ref={this._seek_bar}>
                    <div style={{ float: 'right' }}>
                        {document.fullscreenEnabled ? (
                            <div className="video-controls-button" onClick={this.props.onFullscreenToggle}>
                                <span className="material-symbols-fill">
                                    {this.props.isFullscreen ? 'fullscreen_exit' : 'fullscreen'}
                                </span>
                            </div>
                        ) : null}
                    </div>

                    <div>
                        <div className="video-controls-button" onClick={this.props.onPlayToggle}>
                            <span className="material-symbols-fill">{paused ? 'play_arrow' : 'pause'}</span>
                        </div>
                        {this._render_timecodes()}
                    </div>

                </div>
            </div>

        );
    }
}