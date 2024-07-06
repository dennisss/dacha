import React from "react";
import { CardError } from "./card_error";


export interface CardProps {
    id?: string
    header: any
    error?: any
    style?: any
    onClick?: any
    className?: string
}

interface CardState {
    _open: boolean
}

export class Card extends React.Component<CardProps, CardState> {

    constructor(props: CardProps) {
        super(props);

        let open = true;

        if (this.props.id) {
            let cards_open: any = {};
            try {
                cards_open = JSON.parse(localStorage.cards_open) || {};
            } catch (e) { }

            open = cards_open[props.id];
            if (open === undefined) {
                open = true;
            }
        }

        this.state = {
            _open: open
        };
    }

    _toggle_open = () => {
        if (!this.props.id) {
            return;
        }

        let open = !this.state._open;

        let cards_open: any = {};
        try {
            cards_open = JSON.parse(localStorage.cards_open) || {};
        } catch (e) { }

        cards_open[this.props.id] = open;

        localStorage.cards_open = JSON.stringify(cards_open);

        this.setState({ _open: open });
    }

    render() {
        return (
            <div className={"card" + (this.props.className ? ' ' + this.props.className : '')} style={this.props.style} onClick={this.props.onClick}>
                <div className="card-header" style={{ cursor: this.props.id ? 'pointer' : null }} onClick={this._toggle_open}>
                    {this.props.header}
                </div>
                {this.props.error ? <CardError>{this.props.error}</CardError> : null}
                {this.state._open ? this.props.children : null}
            </div>
        );
    }
}

export class CardBody extends React.Component {
    render() {
        return (
            <div className="card-body">{this.props.children}</div>
        );
    }

}