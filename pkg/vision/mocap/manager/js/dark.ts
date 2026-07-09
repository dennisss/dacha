import { Setting } from "./settings";

class DarkMode {

    constructor() {
        this._setting = new Setting('dark_mode', false);
        this._update();
    }

    set(value: boolean) {
        this._setting.set(value);
        this._update();
    }

    // TODO: All usages of this need a forceUpdate() on changes.
    get() {
        return this._setting.get()
    }

    _update() {
        document.documentElement.setAttribute("data-bs-theme", this.get() ? "dark" : "");
    }
}

export const DARK_MODE = new DarkMode();