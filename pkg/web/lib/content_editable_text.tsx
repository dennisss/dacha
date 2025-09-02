import React from "react";

export interface ContentEditableTextProps {
    value: string,
    onChange: (string) => void
    style?: any
}

export class ContentEditableText extends React.Component<ContentEditableTextProps> {

    _ref: React.RefObject<HTMLDivElement> = React.createRef();

    componentDidMount(): void {
        this._ref.current?.innerText = this.props.value;
    }

    componentDidUpdate(prevProps: Readonly<ContentEditableTextProps>, prevState: Readonly<{}>, snapshot?: any): void {
        if (this._ref.current?.innerText != this.props.value) {
            this._ref.current?.innerText = this.props.value;
        }

    }

    render() {
        return (
            <div ref={this._ref}
                contentEditable="plaintext-only"
                style={this.props.style}
                onInput={(e) => {
                    this.props.onChange(e.target.innerText)
                }}></div>
        );
    }
}