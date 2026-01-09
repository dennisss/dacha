// Written by Google Gemini
// https://gemini.google.com/app/654eeb828bce73c8

/**
 * Draws a centered table with an optional highlighted row.
 * @param {CanvasRenderingContext2D} ctx 
 * @param {number} x - Center X position
 * @param {number} y - Center Y position
 * @param {number} width 
 * @param {number} height 
 * @param {string[]} headers 
 * @param {string[]} flatData 
 * @param {number} highlight_row - Index of the data row to highlight (0-based)
 * @param {number} highlight_alpha - Opacity of the highlight (0.0 to 1.0)
 */
export function drawCenteredTable(ctx, x, y, width, height, headers, flatData, highlight_row, highlight_alpha) {
    const rows = [];
    for (let i = 0; i < flatData.length; i += 2) {
        rows.push([flatData[i], flatData[i + 1]]);
    }

    // Geometry
    const totalRows = rows.length + 1;
    const rowHeight = height / totalRows;
    const colWidth = width / 2;
    const startX = x - (width / 2);
    const startY = y - (height / 2);

    ctx.save();

    // 1. Draw Solid Base (Opaque White)
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(startX, startY, width, height);

    // 2. Draw Row Highlight
    if (highlight_row >= 0 && highlight_row < rows.length) {
        const highlightY = startY + ((highlight_row + 1) * rowHeight);

        ctx.fillStyle = `rgba(255, 0, 0, ${highlight_alpha})`;
        ctx.fillRect(startX, highlightY, width, rowHeight);
    }

    // 3. Draw Header Background
    ctx.fillStyle = "#e0e0e0";
    ctx.fillRect(startX, startY, width, rowHeight);

    // 4. Draw Header Border (The fix)
    // We draw a line exactly at the bottom of the header row
    ctx.lineWidth = 2;
    ctx.strokeStyle = "#000000";

    ctx.beginPath();
    ctx.moveTo(startX, startY + rowHeight);
    ctx.lineTo(startX + width, startY + rowHeight);
    ctx.stroke();

    // 5. Draw Header Text
    ctx.font = "16px 'Noto Sans'";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = "#000000";

    headers.forEach((header, index) => {
        const cellX = startX + (index * colWidth) + (colWidth / 2);
        const cellY = startY + (rowHeight / 2);
        ctx.fillText(header, cellX, cellY);
    });

    // 6. Draw Data Rows and Grid
    ctx.font = "14px 'Noto Sans Mono'";

    rows.forEach((row, rowIndex) => {
        const rowTopY = startY + ((rowIndex + 1) * rowHeight);
        const rowBottomY = rowTopY + rowHeight;

        // Only draw grid lines between data rows or at the very bottom
        // (The header border is handled separately above)
        ctx.beginPath();
        ctx.moveTo(startX, rowBottomY);
        ctx.lineTo(startX + width, rowBottomY);
        ctx.stroke();

        // Text
        row.forEach((text, colIndex) => {
            const cellX = startX + (colIndex * colWidth) + (colWidth / 2);
            const cellY = rowTopY + (rowHeight / 2);
            ctx.fillText(text, cellX, cellY);
        });
    });

    // 7. Outer Border
    ctx.strokeRect(startX, startY, width, height);

    // 8. Vertical Divider
    ctx.beginPath();
    ctx.moveTo(startX + colWidth, startY);
    ctx.lineTo(startX + colWidth, startY + height);
    ctx.stroke();

    ctx.restore();
}