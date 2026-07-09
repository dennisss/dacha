
let SETTINGS = {};

try {
    SETTINGS = JSON.parse(localStorage.getItem('settings')) || {};
} catch {
    SETTINGS = {};
}

function set_setting(name: string, value: any) {
    value = JSON.parse(JSON.stringify(value));
    SETTINGS[name] = value;
    localStorage.setItem('settings', JSON.stringify(SETTINGS));
}

function get_setting(name: string, default_value: any) {
    if (!SETTINGS.hasOwnProperty(name)) {
        set_setting(name, default_value);
    }

    return SETTINGS[name];
}


export class Setting {
    constructor(name, default_value) {
        this._name = name;
        get_setting(name, default_value);
    }

    get() {
        return get_setting(this._name);
    }

    set(value) {
        set_setting(this._name, value);
    }
}