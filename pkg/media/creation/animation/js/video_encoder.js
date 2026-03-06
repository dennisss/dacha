// This code is partially written by Google Gemini.

const { spawn } = require('child_process');

export async function encode_frames(frames, output_path) {
    const args = [
        // '-y', // overwrite without asking
        '-f', 'rawvideo',
        '-pix_fmt', 'bgra',
        '-s', `${frames.width()}x${frames.height()}`,
        '-framerate', frames.rate().toString(),
        '-i', '-',

        // NVENV
        '-c:v', 'h264_nvenc',
        '-preset', 'p4',
        '-cq', '18',

        // x264
        // '-c:v', 'libx264',
        // '-preset', 'veryfast',
        // '-crf', '18',

        '-pix_fmt', 'yuv420p',
        output_path
    ];

    // Spawn the ffmpeg process
    const ffmpeg = spawn('ffmpeg', args);

    ffmpeg.stderr.on('data', (data) => {
        console.error(`[ffmpeg stderr]: ${data}`);
    });

    // Handle process exit
    const ffmpegClosed = new Promise((resolve, reject) => {
        ffmpeg.on('close', (code) => {
            if (code === 0) {
                console.log(`\nffmpeg process exited successfully. Video saved to ${output_path}`);
                resolve();
            } else {
                console.error(`\nffmpeg process exited with code ${code}`);
                reject(new Error(`ffmpeg exited with code ${code}`));
            }
        });

        ffmpeg.on('error', (err) => {
            console.error('Failed to start ffmpeg process.', err);
            reject(err);
        });

        ffmpeg.stdin.on('error', (err) => {
            // This often happens if ffmpeg closes early due to an error
            console.error('Error writing to ffmpeg stdin:', err.message);
        });
    });

    // --- Frame Generation Loop ---

    try {
        console.log('Starting frame generation loop...');
        for (let i = 0; i < frames.length(); i++) {
            const frameBuffer = frames.next();

            const ok = ffmpeg.stdin.write(frameBuffer);

            if (!ok) {
                // Handle backpressure: if the buffer is full, wait for it to drain.
                // This prevents Node.js from buffering frames in memory indefinitely.
                console.log('ffmpeg stdin buffer full, waiting for drain...');
                await new Promise(resolve => ffmpeg.stdin.once('drain', resolve));
            }

            // Update progress in-place
            process.stdout.write(`Processed frame ${i + 1}/${frames.length()}\r`);
        }
    } catch (error) {
        console.error('Error during frame generation loop:', error);
        ffmpeg.kill('SIGINT'); // Kill ffmpeg if our loop fails
    } finally {
        // 3. Close stdin to signal to ffmpeg that we are done sending frames
        console.log('\nAll frames sent. Closing ffmpeg stdin...');
        ffmpeg.stdin.end();
    }

    // Wait for ffmpeg to finish processing and exit
    await ffmpegClosed;
    console.log('Encoding complete.');
}
