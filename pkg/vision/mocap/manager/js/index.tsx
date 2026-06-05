import React from "react";
import ReactDOM from "react-dom";
import { PathParams } from "pkg/web/lib/router";
import { PageContext, PagedApp } from "pkg/web/lib/page";
import { CamerasPage } from "./cameras";
import { WorldPage } from "./world";

const ROUTES = [
    {
        path: '/ui/cameras',
        default: true,
        render: (path: string, params: PathParams, context: PageContext) => {
            return <CamerasPage context={context} />;
        }
    },
    {
        path: '/ui/world',
        render: (path: string, params: PathParams, context: PageContext) => {
            return <WorldPage context={context} />;
        }
    }
]

let node = document.getElementById("app-root");
ReactDOM.render(<PagedApp routes={ROUTES} />, node)