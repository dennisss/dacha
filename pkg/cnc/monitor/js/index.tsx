import React from "react";
import ReactDOM from "react-dom";
import { Router, RouterComponent, PathParams } from "pkg/web/lib/router";
import { FilesPage } from "pkg/cnc/monitor/js/files";
import { MachinePage } from "./machine";
import { MachinesPage } from "./machines";
import { ModalContainerComponent } from "pkg/web/lib/modal";
import { PageComponent, PageContext } from "./page";
import { ProgramRunPage } from "./run";

// TODO: Set a background-color: #fcfcfc on the body.

const ROUTES = [
    {
        path: '/ui/machines',
        default: true,
        render: (path: string, params: PathParams, context: PageContext) => {
            return <MachinesPage context={context} />;
        }
    },
    {
        path: '/ui/files',
        render: (path: string, params: PathParams, context: PageContext) => {
            return <FilesPage context={context} />;
        }
    },
    {
        path: '/ui/machines/:id',
        render: (path: string, params: PathParams, context: PageContext) => {
            return <MachinePage id={params['id']} context={context} />;
        }
    },
    {
        path: '/ui/machines/:machine_id/runs/:run_id',
        render: (path: string, params: PathParams, context: PageContext) => {
            return <ProgramRunPage machine_id={params['machine_id']} run_id={params['run_id']} context={context} />;
        }
    },
]


class App extends React.Component<{}, {}> {

    _router: Router;

    constructor(props: {}) {
        super(props);

        let routes = ROUTES.map((route) => {
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
};




let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)