import { ExponentialBackoff, ExponentialBackoffOptions } from "pkg/net/src/backoff";
import { PageContext } from "./page";
import { Notification } from "pkg/web/lib/notifications";
import { shallow_copy } from "pkg/web/lib/utils";

const BACKOFF_OPTIONS: ExponentialBackoffOptions = {
    base_duration: 1,
    jitter_duration: 2,
    max_duration: 15,
    cooldown_duration: 30,
    max_num_attempts: 0
};

export function watch_entities(ctx: PageContext, request: any, callback: (res: object) => void) {
    let sync_notification: Notification | null = null;
    let backoff = new ExponentialBackoff(BACKOFF_OPTIONS);

    request.watch = true;

    async function sync_loop() {
        while (!ctx.channel.aborted()) {
            // TODO: Cancel on abort. 
            await backoff.start_attempt();

            try {
                await query_attempt();
            } catch (e) {
                set_sync_error(e + '');
            }

            backoff.end_attempt(false);
        }
    }

    function set_sync_error(text: string | null) {
        if (text === null) {
            if (sync_notification !== null) {
                sync_notification.remove();
                sync_notification = null;
            }

            return;
        }

        let full_text = `Out of sync: ${text}`;

        if (sync_notification !== null) {
            sync_notification.update({
                text: full_text
            });
        } else {
            sync_notification = ctx.notifications.add({
                text: full_text,
                preset: 'danger',
                cancellable: false,
            });
        }
    }

    async function query_attempt() {
        let res = ctx.channel.call_streaming('cnc.Monitor', 'QueryEntities', request);

        while (true) {
            let msg = await res.recv();
            if (msg === null) {
                break;
            }

            // Clear error on the first successful sync.
            set_sync_error(null);
            backoff.end_attempt(true);

            callback(msg);
        }

        let status = res.finish();

        if (ctx.channel.aborted()) {
            return;
        }

        set_sync_error(status.toString());
    }

    sync_loop();
}

export async function run_machine_command(ctx: PageContext, machine: any, command: any, done: any) {
    try {
        let req = shallow_copy(command);
        req.machine_id = machine.id;

        let res = await ctx.channel.call('cnc.Monitor', 'RunMachineCommand', req);

        if (!res.status.ok()) {
            throw res.status.toString();
        }

    } catch (e) {
        // TODO: Notification
        console.error(e);
    }

    done();
}

