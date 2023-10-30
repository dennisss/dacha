
https://chromium.googlesource.com/chromium/cdm/+/refs/heads/main/content_decryption_module.h


/opt/google/chrome/WidevineCdm/_platform_specific/linux_x64/libwidevinecdm.so


https://github.com/deadblue/chromium-cdm-proxy




/*
    A DRM example:
    - https://bitmovin.com/demos/drm

Steps:
1. Download
    https://cdn.bitmovin.com/content/assets/art-of-motion_drm/mpds/11331.mpd
    or
    https://storage.googleapis.com/shaka-demo-assets/sintel-widevine/dash.mpd


    https://storage.googleapis.com/shaka-demo-assets/angel-one-widevine/dash.mpd

    https://storage.googleapis.com/shaka-demo-assets/angel-one-clearkey/dash.mpd
    https://storage.googleapis.com/shaka-demo-assets/angel-one/dash.mpd

    Clear keys:
        https://cwip-shaka-proxy.appspot.com/clearkey?_u3wDe7erb7v8Lqt8A3QDQ=ABEiM0RVZneImaq7zN3u_w

        or
        <key>drm.clearKeys.FEEDF00DEEDEADBEEFF0BAADF00DD00D</key>
        <string>00112233445566778899AABBCCDDEEFF</string>

    https://storage.googleapis.com/shaka-demo-assets/bbb-dark-truths-hls/hls.m3u8



2. Use https://cwip-shaka-proxy.appspot.com/no_auth as the Widevine license server

2.



Intercepting EME calls in the browser:

    let originalFunction = MediaKeySession.prototype.generateRequest;
    MediaKeySession.prototype.generateRequest = function(...args) {
        debugger;
        return originalFunction.call(this, ...args);
    }

    let originalFunction2 = MediaKeySession.prototype.update;
    MediaKeySession.prototype.update = function(...args) {
        debugger;
        return originalFunction2.call(this, ...args);
    }

    let originalFunction3 = MediaKeySession.prototype.load;
    MediaKeySession.prototype.load = function(...args) {
        debugger;
        return originalFunction3.call(this, ...args);
    }

    type: "cenc"

    (new Uint8Array(args[1])).toString()

The initialization data is defined here:
- https://w3c.github.io/encrypted-media/format-registry/initdata/cenc.html

Grab the MPD:
-
- Contains the init data in base64

We'd generate a request and send it to
    - https://cwip-shaka-proxy.appspot.com/no_auth



Example code for decrypting it:
- https://github.com/axiomatic-systems/Bento4/blob/master/Source/C%2B%2B/Apps/Mp4Decrypt/Mp4Decrypt.cpp

Key ids for a track are in the "schi" > "tenc" atom
    => Also contains the default constant IV

"senc" atom
    - Contains per sample IVs

Need to read:
    ISO/IEC 23001-7:2016


More MP4 format guidance here:
- https://dashif-documents.azurewebsites.net/DASH-IF-IOP/master/DASH-IF-IOP.html#CPS-ISO4CENC
*/
