import React from "react";
import { SpinnerInline } from "pkg/web/lib/spinner";
import { shallow_copy } from "pkg/web/lib/utils";


// Stores all of the notifications that we should actively show to users.
export class NotificationStore {
    _last_notification_id: number = 0;
    _notifications: { [id: number]: NotificationEntry } = {};
    _listeners: (() => void)[] = []

    constructor() { }

    add(data: NotificationData): Notification {
        let id = this._last_notification_id + 1;
        this._last_notification_id = id;

        let entry = {
            id: id,
            last_updated: new Date(),
            data: data
        };

        this._notifications[id] = entry;

        this._notify_all();

        return new Notification(this, entry);
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

    _remove(id: number) {
        let obj = shallow_copy(this._notifications);
        delete obj[id];

        this._notifications = obj;
        this._notify_all();
    }

    _notify_all() {
        this._listeners.map((f) => {
            f();
        });
    }
}

interface NotificationEntry {
    id: number,
    last_updated: Date,
    data: NotificationData
}

export interface NotificationData {
    text: string,
    preset: string,
    cancellable: boolean,
}

export interface NotificationDataObject {
    text?: string
    preset?: string
    cancellable?: boolean
}

export class Notification {
    _store: NotificationStore;
    _entry: NotificationEntry;

    constructor(store: NotificationStore, entry: NotificationEntry) {
        this._store = store;
        this._entry = entry;
    }

    update(new_data: NotificationDataObject) {
        this._entry.last_updated = new Date();

        // TODO: Copy all the way the way up to this._store._notification so that it appears as un-equal.
        let data = shallow_copy(this._entry.data);
        Object.assign(data, new_data);
        this._entry.data = data;

        this._store._notify_all();
    }

    // Removes this notification
    remove() {
        this._store._remove(this._entry.id);
    }

}


export interface NotificationsComponentProps {
    notifications: NotificationStore
}

export class NotificationsComponent extends React.Component<NotificationsComponentProps> {

    constructor(props: NotificationsComponentProps) {
        super(props);
        this.props.notifications.add_listener(this._on_change);
    }

    componentWillUnmount(): void {
        this.props.notifications.remove_listener(this._on_change);
    }

    _on_change = () => {
        this.forceUpdate();
    }

    render() {
        let store = this.props.notifications;

        let entries = Object.values(store._notifications);
        entries.sort((a, b) => {
            // TODO: If equal, compare ids.
            return b.last_updated.getTime() - a.last_updated.getTime();
        });

        // TODO: We should use fixed positioning for these.
        return (
            <div className="toast-container position-fixed p-3 top-0 start-50 translate-middle-x" style={{ zIndex: 1000 }}>
                {entries.map((entry) => {
                    return (
                        <div key={entry.id} className={`toast align-items-center border-0 bg-${entry.data.preset} text-white fade show`}>
                            <div className="d-flex">
                                <div className="toast-body">
                                    {entry.data.text}
                                </div>
                                {entry.data.cancellable ? (
                                    <button type="button" className="btn-close btn-close-white me-2 m-auto"
                                        onClick={() => store._remove(entry.id)}></button>
                                ) : (
                                    <div className="me-2 m-auto">
                                        <SpinnerInline />
                                    </div>
                                )}
                            </div>

                        </div>
                    );
                })}

            </div>

        );
    }
}
