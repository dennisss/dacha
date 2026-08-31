const { setGlobalOptions } = require("firebase-functions");
const { onRequest } = require("firebase-functions/https");
const { Storage } = require('@google-cloud/storage');
const logger = require("firebase-functions/logger");
const path = require('path');

setGlobalOptions({
    maxInstances: 10,
    serviceAccount: "firebase-functions@da-cha.iam.gserviceaccount.com"
});

const storage = new Storage();

const TARGET_BUCKET_NAME = 'da-sources';
const CONFIG_BUCKET_NAME = 'da-sources';
const CONFIG_FILE_PATH = 'files.json';

let filesMapCache = null;
let lastCacheTime = 0;
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes

async function getFilesMap() {
    // Return cached version if valid
    if (filesMapCache && (Date.now() - lastCacheTime < CACHE_TTL_MS)) {
        return filesMapCache;
    }

    // Otherwise, download from GCS
    const bucket = storage.bucket(CONFIG_BUCKET_NAME);
    const file = bucket.file(CONFIG_FILE_PATH);
    const [contents] = await file.download();

    filesMapCache = JSON.parse(contents.toString('utf8'));
    lastCacheTime = Date.now();

    return filesMapCache;
}

exports.downloadSourceFile = onRequest(async (req, res) => {
    try {
        const reqPath = req.path.replace(/^\/+/, '');
        const filesMap = await getFilesMap();

        const fileConfig = filesMap.files.find(f => f.path.replace(/^\/+/, '') === reqPath);

        if (!fileConfig) {
            return res.status(404).send('File not found');
        }

        const bucket = storage.bucket(TARGET_BUCKET_NAME);
        const file = bucket.file(fileConfig.bucket_path);
        const downloadFilename = path.basename(fileConfig.path);

        const [signedUrl] = await file.getSignedUrl({
            version: 'v4',
            action: 'read',
            expires: Date.now() + 15 * 60 * 1000,
            responseDisposition: `attachment; filename="${downloadFilename}"`
        });

        res.redirect(302, signedUrl);

    } catch (error) {
        logger.error('Error processing request:', error);
        res.status(500).send('Internal Server Error');
    }
});