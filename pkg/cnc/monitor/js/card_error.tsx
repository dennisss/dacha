import React from "react";


export class CardError extends React.Component {
    render() {
        return (
            <div className="bg-warning" style={{ padding: '6px 16px' }}>
                <span className="material-symbols-outlined" style={{ verticalAlign: 'bottom' }}>error</span>
                &nbsp;
                {this.props.children}
            </div>
        );
    }
}