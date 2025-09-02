import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { NotificationStore, NotificationsComponent } from "pkg/web/lib/notifications";
import { Route, Router, RouterComponent } from "pkg/web/lib/router";
import { ModalContainerComponent } from "pkg/web/lib/modal";

export interface PageContext {
    channel: Channel;
    notifications: NotificationStore;
}

export interface PagedAppProps {
    routes: Route[]
}

// Outer most component to use in an app with multiple pages.
//
// NOTE: This assumes that the set of routes won't change.
export class PagedApp extends React.Component<PagedAppProps> {
    _router: Router;

    constructor(props: PagedAppProps) {
        super(props);

        let routes = props.routes.map((route) => {
            let inner_render = route.render;
            route.render = (path, params) => {
                return <PageComponent key={path} render={(context) => inner_render(path, params, context)} />
            }

            return route;
        })

        this._router = new Router(routes);
    }

    render() {
        return (
            <div className="app-outer">
                <RouterComponent router={this._router} />
                <ModalContainerComponent />
            </div>
        );
    }
}

interface PageComponentProps {
    render: (context: PageContext) => React.ReactNode
    key: string
}

// NOTE: This must be used along with a 'key' to ensure that it is unmounted on route changes.
class PageComponent extends React.Component<PageComponentProps> {

    _abort_controller: AbortController;
    _context: PageContext;

    constructor(props: PageComponentProps) {
        super(props);

        // TODO: Dedup this code.
        let channel = new Channel('/rpc');

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
