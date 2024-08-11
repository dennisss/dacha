

/*
Handling page visibility changes:

- For VOD:
    - Equivalent of pause:
        - Allow any ongoing requests to finish if they can
        - Don't issue new requests or 

- For Live video
    - When we gide
        - Pause and stop requests
    - Wehn 

*/


function ranges_overlap(a: Range, b: Range): boolean {
    if (a.start == b.start) {
        return true;
    } else if (a.start < b.start) {
        return a.end > b.start;
    } else { // a.start > b.start
        return a.start < b.end;
    }
}







