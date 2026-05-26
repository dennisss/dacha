import React from "react";
import ReactDOM from "react-dom";
import { PathParams } from "pkg/web/lib/router";
import { PageContext, PagedApp } from "pkg/web/lib/page";
import { CamerasPage } from "./cameras";

const ROUTES = [
    {
        path: '/ui/cameras',
        default: true,
        render: (path: string, params: PathParams, context: PageContext) => {
            return <CamerasPage context={context} />;
        }
    },
    // {
    //     path: '/ui/space',
    //     render: (path: string, params: PathParams, context: PageContext) => {
    //         return <FilesPage context={context} />;
    //     }
    // }
]

let node = document.getElementById("app-root");
ReactDOM.render(<PagedApp routes={ROUTES} />, node)