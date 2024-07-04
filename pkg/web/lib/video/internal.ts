import { ExponentialBackoffOptions } from "pkg/net/src/backoff";

export const BACKOFF_OPTIONS: ExponentialBackoffOptions = {
    base_duration: 1,
    jitter_duration: 2,
    max_duration: 15,
    cooldown_duration: 30,
    max_num_attempts: 0
};
