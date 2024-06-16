import React from "react";
import { shallow_copy } from "./utils";

export type PathParams = { [key: string]: string };

// Each route is of an object of the form:
// { default: bool, path: "/something/:param/..." render: function(params) -> Element }
export interface Route {
    default?: boolean,
    path: string,
    render: any
}

class ResolvedRoute {
    route: Route;
    params: PathParams;
}

let GLOBAL_ROUTER: Router | null = null;

export class Router {
    static global(): Router {
        if (GLOBAL_ROUTER === null) {
            throw new Error('No global router initialized yet');
        }

        return GLOBAL_ROUTER;
    }

    _routes: Route[];
    _subscribers: any[];

    constructor(routes: Route[]) {
        this._routes = routes;
        this._subscribers = []

        this._goto_replace();

        // This will be called whenever the user navigates forward/backward in their browser history stack.
        window.addEventListener('popstate', (e) => {
            this._goto_replace();
        });

        GLOBAL_ROUTER = this;
    }

    current_path(): string {
        return window.location.pathname;
    }

    routes(): Route[] {
        return this._routes;
    }

    resolve(path: string): ResolvedRoute | null {
        let selected_route = null;
        let selected_params = null;
        for (let i = 0; i < this._routes.length; i++) {
            let route = this._routes[i];
            if ((!path || path == '/') && route.default) {
                selected_route = route;
                break;
            }

            let params = match_path_pattern(path, route.path);
            if (params === null) {
                continue;
            }

            selected_route = route;
            selected_params = params;
            break;
        }

        if (selected_route === null) {
            return null;
        }

        let out = new ResolvedRoute();
        out.params = selected_params || {};
        out.route = selected_route;
        return out;
    }

    add_change_listener(listener: any) {
        this._subscribers.push(listener);
    }

    goto(path: string) {
        // Normalize the path.
        let resolved = this.resolve(path)
        if (resolved !== null) {
            path = create_path_from_params(resolved.params, resolved.route.path);
        }

        window.history.pushState(null, '', path);

        this._subscribers.map((s) => {
            s();
        });
    }

    _goto_replace() {
        let path = this.current_path();
        let resolved = this.resolve(path)
        if (resolved !== null) {
            let normalized_path = create_path_from_params(resolved.params, resolved.route.path);
            if (normalized_path !== path) {
                window.history.replaceState(null, '', normalized_path);
            }
        }

        this._subscribers.map((s) => {
            s();
        });
    }
}


// Splits a uri path into individual components.
// e.g. split_path('/hello/world') == ['hello', 'world'] 
function split_path(path: string): string[] {
    if (!path) {
        return [];
    }

    let parts = path.split('/');
    for (let i = 0; i < parts.length; ++i) {
        if (parts[i].trim().length == 0) {
            parts.splice(i, 1);
            i--;
        }
    }

    return parts;
}

function match_path_pattern(path: string, pattern: string): PathParams | null {
    let path_parts = split_path(path);
    let pattern_parts = split_path(pattern);

    if (path_parts.length != pattern_parts.length) {
        return null;
    }

    let params: PathParams = {}
    for (let i = 0; i < pattern_parts.length; ++i) {
        if (pattern_parts[i].charAt(0) == ':') {
            let name = pattern_parts[i].slice(1);
            let value = decodeURIComponent(path_parts[i]);
            params[name] = value;
        } else {
            if (pattern_parts[i] != path_parts[i]) {
                return null;
            }
        }
    }

    return params;
}

// TODO: USe this.
function create_path_from_params(params: PathParams, pattern: string): string {
    let params_mut = shallow_copy(params);
    let pattern_parts = split_path(pattern);

    let path_parts = []
    for (let i = 0; i < pattern_parts.length; i++) {
        if (pattern_parts[i].charAt(0) == ':') {
            let name = pattern_parts[i].slice(1);
            if (!params_mut[name]) {
                throw new Error('Unspecified value for path parameter: ' + name);
            }

            let value = encodeURIComponent(params_mut[name]);
            path_parts.push(value);

            delete params_mut[name];
        } else {
            path_parts.push(pattern_parts[i]);
        }
    }

    if (Object.keys(params_mut).length != 0) {
        throw new Error('Unused parameters while building path');
    }

    return '/' + path_parts.join('/');
}


export interface RouterComponentProps {
    router: Router
}


interface RouterComponentState {
    path: string
}

/// Top level component which will select a sub-component to render depending on the current route.
export class RouterComponent extends React.Component<RouterComponentProps, RouterComponentState> {

    constructor(props: RouterComponentProps) {
        super(props);
        props.router.add_change_listener(this._on_location_change);
        this.state = {
            path: props.router.current_path()
        };
    }

    _on_location_change = () => {
        this.setState({ path: this.props.router.current_path() });
    }

    render() {
        let resolved = this.props.router.resolve(this.state.path);

        if (resolved === null) {
            throw new Error('No route for the current path');
        }

        return (resolved.route.render)(this.state.path, resolved.params);
    }

}

export interface LinkProps {
    to: string
}

export class Link extends React.Component<LinkProps> {
    _on_click = (e) => {
        e.preventDefault();
        Router.global().goto(this.props.to);
    }

    render() {
        return <a href="#" onClick={this._on_click}>{this.props.children}</a>
    }
}