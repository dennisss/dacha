import { FigureLegendEntry } from "pkg/web/lib/figure/legend";
import { CarveraLevelingState } from "./carvera";
import { BinaryImageData } from "./binary_image";


// Shared state 
export class MachineUiState {
    _listeners: (() => void)[] = []
    _jog_feed_rate: number = 1000;
    _position_legend: FigureLegendState;

    _carvera_state: CarveraLevelingState | null = null;
    _program_preview: ProgramPreviewContainer | null = null;


    constructor() {
        this._position_legend = new FigureLegendState(this);
    }

    jog_feed_rate() {
        return this._jog_feed_rate
    }

    set_job_feed_rate(v: number) {
        this._jog_feed_rate = v;
        this._notify_all();
    }

    // NOTE: The legend state is populated on the first render of the position graph.
    position_legend(): FigureLegendState {
        return this._position_legend
    }

    carvera_state(): CarveraLevelingState | null {
        return this._carvera_state;
    }

    set_carvera_state(state: CarveraLevelingState | null) {
        this._carvera_state = state;
        this._notify_all();
    }

    program_preview(): ProgramPreviewContainer | null {
        return this._program_preview;
    }

    set_program_preview(data: ProgramPreviewContainer | null) {
        this._program_preview = data;
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
};

export class FigureLegendState {

    constructor(root: MachineUiState) {
        this._root = root;
    }

    _root: MachineUiState;
    _entries: Map<string, FigureLegendEntry> = new Map();

    // NOTE: This is meant to be called fom the PositionBox so doesn't trigger listeners.
    get_or_insert(new_entry: FigureLegendEntry): FigureLegendEntry {
        let v = this._entries.get(new_entry.id);
        if (v !== undefined) {
            return v;
        }

        v = new_entry;
        this._entries.set(v.id, v);
        return v;
    }

    get(id: string): FigureLegendEntry | undefined {
        return this._entries.get(id);
    }

    set(entry: FigureLegendEntry) {
        this._entries.set(entry.id, entry);
        this._root._notify_all();
    }
}

export interface ProgramPreviewContainer {
    file_id: any;
    config_key: any;
    revision: any;

    error?: string;
    loading?: boolean;
    data?: ProgramPreviewData
}

export interface ProgramPreviewData {
    layer_images: BinaryImageData[];
}


