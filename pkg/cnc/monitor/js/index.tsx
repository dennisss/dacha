import React from "react";
import ReactDOM from "react-dom";
import { PathParams } from "pkg/web/lib/router";
import { FilesPage } from "pkg/cnc/monitor/js/files";
import { MachinePage } from "./machine";
import { MachinesPage } from "./machines";
import { PageContext, PagedApp } from "pkg/web/lib/page";
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

let node = document.getElementById("app-root");
ReactDOM.render(<PagedApp routes={ROUTES} />, node)