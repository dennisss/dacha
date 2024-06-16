import { Channel } from "pkg/web/lib/rpc";
import { NotificationStore, NotificationsComponent } from "pkg/web/lib/notifications";
import React from "react";

export interface PageContext {
    channel: Channel;
    notifications: NotificationStore;
}

export interface PageComponentProps {
    render: (context: PageContext) => React.ReactNode
    key: string
}

// NOTE: This must be used along with a 'key' to ensure that it is unmounted on route changes.
export class PageComponent extends React.Component<PageComponentProps> {

    _abort_controller: AbortController;
    _context: PageContext;

    constructor(props: PageComponentProps) {
        super(props);

        let channel = new Channel("http://localhost:8001");
        this._abort_controller = new AbortController();
        channel.add_abort_signal(this._abort_controller.signal);

        this._context = {
            channel: channel,
            notifications: new NotificationStore()
        };
    }

    componentWillUnmount(): void {
        this._abort_controller.abort();
    }

    render() {
        return (
            <div className="app-page">
                <NotificationsComponent notifications={this._context.notifications} />
                {(this.props.render)(this._context)}
            </div>
        );
    }
}