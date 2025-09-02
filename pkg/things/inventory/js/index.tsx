import React from "react";
import ReactDOM from "react-dom";
import { PathParams } from "pkg/web/lib/router";
import { PageContext, PagedApp } from "pkg/web/lib/page";
import { PartsPage } from "./parts";
import { PacksPage } from "./packs";

const ROUTES = [
    {
        path: '/ui/packs',
        default: true,
        render: (path: string, params: PathParams, context: PageContext) => {
            return <PacksPage context={context} />;
        }
    },
    {
        path: '/ui/parts',
        render: (path: string, params: PathParams, context: PageContext) => {
            return <PartsPage context={context} />;
        }
    },
]

let node = document.getElementById("app-root");
ReactDOM.render(<PagedApp routes={ROUTES} />, node)