

export function maybe_redirect_to_referer(): boolean {
    try {
        let search_params = new URLSearchParams(window.location.search);
        let referer = search_params.get('referer');
        if (referer) {
            // Remove from the referer from the current page in the
            // history so that the user can hit the back button to see
            // their profile.
            window.history.replaceState(null, '', '/');

            window.location.href = referer;

            return true;
        }
    } catch (e) {
        console.error(e);
    }

    return false;
}