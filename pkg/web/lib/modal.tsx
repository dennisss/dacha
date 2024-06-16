import React from "react";


/*
TODO: On the <body> we need to add:
overflow: hidden
*/

let MODAL_STORE: ModalStore | null = null;

export class ModalStore {

    static global(): ModalStore {
        if (MODAL_STORE === null) {
            MODAL_STORE = new ModalStore();
        }

        return MODAL_STORE;
    }

    _listeners: (() => void)[] = [];
    _current: ModalOptions | null = null;

    open(options: ModalOptions) {
        this._current = options;
        this._notify_all();
    }

    close() {
        this._current = null;
        this._notify_all();
    }

    add_listener(f: () => void) {
        this._listeners.push(f);
    }

    remove_listener(f: () => void) {
        for (let i = 0; i < this._listeners.length; i++) {
            if (this._listeners[i] == f) {
                this._listeners.splice(i, 1);
                break;
            }
        }
    }

    _notify_all() {
        this._listeners.map((f) => {
            f();
        });
    }
}

export interface ModalOptions {
    title: string
    body: any
    dialog_style?: any
    content_style?: any
}

// Component which stores all of the 
export class ModalContainerComponent extends React.Component {

    constructor(props: any) {
        super(props);
        ModalStore.global().add_listener(this._on_change);
    }

    componentWillUnmount(): void {
        ModalStore.global().remove_listener(this._on_change);
    }

    _on_change = () => {
        this.forceUpdate();
    }

    _on_close = () => {
        ModalStore.global().close();
    }

    render() {
        let store = ModalStore.global();
        if (!store._current) {
            return <div></div>;
        }

        let options = store._current;

        return (
            <div>
                <div className="modal-backdrop fade show"></div>
                <ModalComponent onClose={this._on_close} options={options} />
            </div>
        );
    }

}

interface ModalComponentProps {
    onClose: any
    options: ModalOptions
}

class ModalComponent extends React.Component<ModalComponentProps> {
    render() {
        return (
            <div className="modal" tabIndex={-1} style={{ display: 'block' }} onClick={this.props.onClose}>
                <div className="modal-dialog" onClick={(e) => e.stopPropagation()} style={this.props.options.dialog_style}>
                    <div className="modal-content" style={this.props.options.content_style}>
                        <div className="modal-header" style={{ backgroundColor: '#fafafa' }}>
                            <h5 className="modal-title">{this.props.options.title}</h5>
                            <button type="button" className="btn-close" onClick={this.props.onClose}></button>
                        </div>
                        {this.props.options.body}

                        {/*
                        <div className="modal-footer">
                            <button type="button" className="btn btn-secondary" data-bs-dismiss="modal">Close</button>
                            <button type="button" className="btn btn-primary">Save changes</button>
                        </div>
                        */}
                    </div>
                </div>
            </div>
        );
    }
}

interface ModalBodyProps {
    style: any
}

export class ModalBody extends React.Component<ModalBodyProps> {
    render() {
        return (
            <div className="modal-body" style={this.props.style}>
                {this.props.children}
            </div>
        );
    }
}
