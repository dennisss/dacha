
// Duration units are in seconds.
export interface ExponentialBackoffOptions {
    base_duration: number
    jitter_duration: number
    max_duration: number
    cooldown_duration: number
    max_num_attempts: number
}

export class ExponentialBackoff {
    _options: ExponentialBackoffOptions
    _current_backoff: number = 0;
    _successful_since: Date | null = null;
    _last_completion: Date | null = null;
    _attempt_count: number = 0;
    _attempt_pending: boolean = false;

    constructor(options: ExponentialBackoffOptions) {
        this._options = options;
    }

    async start_attempt() {
        if (this._attempt_pending) {
            this.end_attempt(false);
        }

        if (this._options.max_num_attempts > 0 && this._attempt_count >= this._options.max_num_attempts) {
            throw new Error('Exceeded maximum number of attempts');
        }

        this._attempt_pending = true;
        if (this._options.max_num_attempts > 0) {
            this._attempt_count += 1;
        }

        if (this._current_backoff == 0) {
            // No backoff
            return;
        }

        let wait_time = this._current_backoff + (Math.random() * this._options.jitter_duration);

        let now = new Date();
        if (this._last_completion !== null) {
            let elapsed = (now.getTime() - this._last_completion.getTime()) / 1000;
            if (elapsed >= wait_time) {
                return;
            }

            if (elapsed > 0) {
                wait_time -= elapsed;
            }
        }

        await new Promise((res, _) => {
            setTimeout(() => res(null), wait_time * 1000);
        });
    }

    end_attempt(successful: boolean) {
        let now = new Date();

        this._attempt_pending = false;
        this._last_completion = now;

        if (this._successful_since !== null) {
            if ((now.getTime() - this._successful_since.getTime()) / 1000 > this._options.cooldown_duration) {
                this._current_backoff = 0;
            }
        }

        if (successful) {
            this._attempt_count = 0;
            if (this._successful_since == null) {
                this._successful_since = now;
            }
        } else {
            if (this._current_backoff == 0) {
                this._current_backoff = this._options.base_duration;
            } else {
                this._current_backoff = Math.min(
                    2 * this._current_backoff,
                    this._options.max_duration
                );
            }

            this._successful_since = null;
        }
    }
}