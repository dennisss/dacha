// Made by Google Gemini
// https://gemini.google.com/app/c06d25ba000d06c2

export function drawShearedSquare(ctx, center, length, shearX, curvatureX, progress, color, text = '20mm') {
    // 1. Generate the Path Points (Visible Shape)
    const points = generateAsymmetricPoints(center, length, shearX, curvatureX);

    // 2. Draw the visible shape
    ctx.lineWidth = 3;
    ctx.strokeStyle = color;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    drawPolyline(ctx, points, progress);

    // 3. Calculate Reference Anchors (Theoretical Shape at Curvature 0)
    // We use the full original length to anchor dimensions.
    const halfLen = length / 2;
    const halfShear = shearX / 2;

    // Theoretical Top-Left and Top-Right (Standard Parallelogram)
    const refTL = {
        x: center.x - halfLen + halfShear,
        y: center.y - halfLen
    };

    const refTR = {
        x: center.x + halfLen + halfShear,
        y: center.y - halfLen
    };

    // 4. Draw Dimensions
    const offsetDist = 20;

    // -- X Dimension --
    // Extensions follow shear vector, anchored to RefTL and RefTR
    const shearVec = { x: shearX, y: -length };
    const shearLen = Math.hypot(shearVec.x, shearVec.y);
    const extDir = { x: shearVec.x / shearLen, y: shearVec.y / shearLen };

    drawSlantedHorizontalDim(ctx, refTL, refTR, extDir, offsetDist, text);

    // -- Y Dimension --
    // Anchor to the leftmost X of the Reference shape
    const refBL_x = center.x - halfLen - halfShear;
    const leftmostX = Math.min(refTL.x, refBL_x);

    const anchorTop = { x: leftmostX, y: center.y - halfLen };
    const anchorBot = { x: leftmostX, y: center.y + halfLen };

    drawVerticalDim(ctx, anchorTop, anchorBot, -1, offsetDist, text);
}

function generateAsymmetricPoints(center, length, shearX, curveDeflection) {
    const points = [];
    const steps = 40;

    const halfLen = length / 2;
    const halfShear = shearX / 2;

    // DEFINING BASE POSITIONS (Relative to center X)
    // Left Base: Fixed at -length/2
    // Right Base: Moved inward by 1 * curveDeflection
    const leftBaseX = -length / 2;
    const rightBaseX = (length / 2) - curveDeflection;

    // Helper to calculate transformed point
    // xBase is either leftBaseX or rightBaseX
    const getPoint = (xBase, yNorm) => {
        // 1. Linear Shear (Top moves Right, Bottom moves Left)
        const shearOffset = -yNorm * halfShear;

        // 2. Curvature (Cosine wave, parallel shift for both sides)
        // Deflects RIGHT (positive X) in the middle
        const curveOffset = curveDeflection * Math.cos(yNorm * Math.PI / 2);

        return {
            x: center.x + xBase + shearOffset + curveOffset,
            y: center.y + (yNorm * halfLen)
        };
    };

    // Top Edge (Straight)
    // From Left Base to Right Base at yNorm = -1
    points.push(getPoint(leftBaseX, -1));
    points.push(getPoint(rightBaseX, -1));

    // Right Edge (Curve)
    // From yNorm -1 to 1 using Right Base
    for (let i = 1; i <= steps; i++) {
        const yNorm = -1 + (2 * i / steps);
        points.push(getPoint(rightBaseX, yNorm));
    }

    // Bottom Edge (Straight)
    // From Right Base to Left Base at yNorm = 1
    // (Note: The last point of Right Edge was getPoint(rightBaseX, 1), so we move to Left Base)
    points.push(getPoint(leftBaseX, 1));

    // Left Edge (Curve)
    // From yNorm 1 to -1 using Left Base
    for (let i = 1; i <= steps; i++) {
        const yNorm = 1 - (2 * i / steps);
        points.push(getPoint(leftBaseX, yNorm));
    }

    return points;
}

export function drawPolyline(ctx, points, progress) {
    if (points.length < 2) return;
    let totalLen = 0;
    const dists = [];
    for (let i = 0; i < points.length - 1; i++) {
        const d = Math.hypot(points[i + 1].x - points[i].x, points[i + 1].y - points[i].y);
        dists.push(d);
        totalLen += d;
    }
    const closeDist = Math.hypot(points[0].x - points[points.length - 1].x, points[0].y - points[points.length - 1].y);
    dists.push(closeDist);
    totalLen += closeDist;

    let drawLen = totalLen * progress;

    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);

    for (let i = 0; i < points.length; i++) {
        const nextPt = (i === points.length - 1) ? points[0] : points[i + 1];
        const segDist = dists[i];

        if (drawLen > segDist) {
            ctx.lineTo(nextPt.x, nextPt.y);
            drawLen -= segDist;
        } else {
            if (drawLen > 0) {
                const ratio = drawLen / segDist;
                const interpX = points[i].x + (nextPt.x - points[i].x) * ratio;
                const interpY = points[i].y + (nextPt.y - points[i].y) * ratio;
                ctx.lineTo(interpX, interpY);
            }
            break;
        }
    }
    ctx.stroke();
}

/**
 * Draws multiple non-closed chains of segments sequentially.
 * @param {CanvasRenderingContext2D} ctx 
 * @param {Array<Array<{x: number, y: number}>>} chains - List of point lists
 * @param {number} progress - 0.0 to 1.0
 */
export function drawSequentialChains(ctx, chains, progress) {
    // 1. Calculate Total Length of all chains combined
    let totalLength = 0;
    const chainLengths = []; // Store segment lengths for efficiency

    for (let i = 0; i < chains.length; i++) {
        const chain = chains[i];
        const segs = [];
        // A chain needs at least 2 points to form a segment
        if (chain.length > 1) {
            for (let j = 0; j < chain.length - 1; j++) {
                const dist = Math.hypot(
                    chain[j + 1].x - chain[j].x,
                    chain[j + 1].y - chain[j].y
                );
                totalLength += dist;
                segs.push(dist);
            }
        }
        chainLengths.push(segs);
    }

    // 2. Determine drawing budget
    let remaining = totalLength * progress;

    ctx.beginPath();

    // 3. Draw
    for (let i = 0; i < chains.length; i++) {
        const chain = chains[i];
        const segs = chainLengths[i];

        if (chain.length < 2) continue;

        // Stop if we have no length left to draw
        // (Use small epsilon to avoid floating point issues)
        if (remaining <= 0.001) break;

        // Move to the start of this specific chain
        ctx.moveTo(chain[0].x, chain[0].y);

        for (let j = 0; j < chain.length - 1; j++) {
            const p1 = chain[j];
            const p2 = chain[j + 1];
            const dist = segs[j];

            if (remaining >= dist) {
                // Draw full segment
                ctx.lineTo(p2.x, p2.y);
                remaining -= dist;
            } else {
                // Draw partial segment
                const ratio = remaining / dist;
                const ix = p1.x + (p2.x - p1.x) * ratio;
                const iy = p1.y + (p2.y - p1.y) * ratio;
                ctx.lineTo(ix, iy);
                remaining = 0;
                break; // Stop this chain
            }
        }
    }

    ctx.stroke();
}

function drawSlantedHorizontalDim(ctx, p1, p2, extDir, offset, labelText) {
    const gap = 8;

    const k = -offset / extDir.y;
    const dimY = p1.y - offset;

    const x1_dim = p1.x + (k * extDir.x);
    const x2_dim = p2.x + (k * extDir.x);

    ctx.lineWidth = 1;
    ctx.strokeStyle = "#666";
    ctx.fillStyle = "#666";
    ctx.font = '18px "Noto Sans"';
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    // Extension 1
    ctx.beginPath();
    ctx.moveTo(p1.x + (gap * extDir.x), p1.y + (gap * extDir.y));
    ctx.lineTo(x1_dim + (10 * extDir.x), dimY + (10 * extDir.y));
    ctx.stroke();

    // Extension 2
    ctx.beginPath();
    ctx.moveTo(p2.x + (gap * extDir.x), p2.y + (gap * extDir.y));
    ctx.lineTo(x2_dim + (10 * extDir.x), dimY + (10 * extDir.y));
    ctx.stroke();

    // Main Line
    ctx.beginPath();
    ctx.moveTo(x1_dim, dimY);
    ctx.lineTo(x2_dim, dimY);
    ctx.stroke();

    // Text Label
    const midX = (x1_dim + x2_dim) / 2;
    const midY = dimY;

    ctx.save();
    ctx.translate(midX, midY - 15);

    const textWidth = ctx.measureText(labelText).width;
    ctx.fillStyle = "#fdfdfd";
    ctx.fillRect(-textWidth / 2 - 2, -8, textWidth + 4, 16);
    ctx.fillStyle = "#666";
    ctx.fillText(labelText, 0, 0);
    ctx.restore();
}

function drawVerticalDim(ctx, p1, p2, dirMultiplier, offset, labelText) {
    const gap = 8;
    const extensionPast = 10;

    const xBase = p1.x + (offset * dirMultiplier);

    ctx.lineWidth = 1;
    ctx.strokeStyle = "#666";
    ctx.fillStyle = "#666";
    ctx.font = '18px "Noto Sans"';
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    // Extensions
    ctx.beginPath();
    ctx.moveTo(p1.x + (gap * dirMultiplier), p1.y);
    ctx.lineTo(xBase + (extensionPast * dirMultiplier), p1.y);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(p2.x + (gap * dirMultiplier), p2.y);
    ctx.lineTo(xBase + (extensionPast * dirMultiplier), p2.y);
    ctx.stroke();

    // Main Line
    ctx.beginPath();
    ctx.moveTo(xBase, p1.y);
    ctx.lineTo(xBase, p2.y);
    ctx.stroke();

    // Text
    ctx.save();
    ctx.translate(xBase - 15, (p1.y + p2.y) / 2);
    ctx.rotate(-Math.PI / 2);
    const textWidth = ctx.measureText(labelText).width;
    ctx.fillStyle = "#fdfdfd";
    ctx.fillRect(-textWidth / 2 - 2, -8, textWidth + 4, 16);
    ctx.fillStyle = "#666";
    ctx.fillText(labelText, 0, 0);
    ctx.restore();
}