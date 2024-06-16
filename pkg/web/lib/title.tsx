import React from "react";


export class Title extends React.Component<{ value: string }> {

    componentDidMount() {
        document.getElementsByTagName('title')[0].innerText = this.props.value;
    }

    componentDidUpdate() {
        this.componentDidMount();
    }

    render() {
        return null;
    }
}