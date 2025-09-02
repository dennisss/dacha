import React from "react";

export class VideoCrosshair extends React.Component<{ size: number | null }> {
    render() {
        let size = this.props.size || 0;
        if (size <= 0) {
            return null;
        }

        return (
            <>
                <div style={{ width: size, height: 1, position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', backgroundColor: 'red' }}></div>
                <div style={{ width: 1, height: size, position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', backgroundColor: 'red' }}></div>
                <div style={{ width: size, height: size, position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', border: '1px solid red', borderRadius: size / 2 }}></div>
            </>
        );
    }
}