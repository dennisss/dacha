import React from "react";
import { PageContext } from "../page";


export class CameraBox extends React.Component<{ machine: any, context: PageContext }> {

    render() {
        {/*
        If camera present:
        - show and allow disconnecting it.

        Else:
        - allow opening a list of available cameras to pick from.

        */}

        return (
            <div style={{ width: '100%', height: '200px', backgroundColor: '#ccc', marginBottom: 10 }}>

            </div>
        );
    }
};

