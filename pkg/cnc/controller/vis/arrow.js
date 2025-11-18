// This is written by Google Gemini.
// https://gemini.google.com/app/50af03c66cb55e84

export function drawArrow(context, fromX, fromY, toX, toY, lineWidth, headSize, hasReverseArrow = false) {

    const dx = toX - fromX;
    const dy = toY - fromY;
    const angle = Math.atan2(dy, dx);

    // Calculate the offset for the arrowhead "height"
    const offset = headSize * Math.cos(Math.PI / 6);

    // --- Calculate the line start and end points ---

    // Shorten the line start if there's a reverse arrow
    const lineStartX = fromX + (hasReverseArrow ? offset * Math.cos(angle) : 0);
    const lineStartY = fromY + (hasReverseArrow ? offset * Math.sin(angle) : 0);

    // Shorten the line end for the forward arrow
    const lineEndX = toX - offset * Math.cos(angle);
    const lineEndY = toY - offset * Math.sin(angle);

    context.save(); // Save context state

    // --- Draw the line ---
    context.beginPath();
    context.moveTo(lineStartX, lineStartY);
    context.lineTo(lineEndX, lineEndY);
    context.lineWidth = lineWidth;
    context.stroke();

    // --- Draw the forward arrowhead (at the "to" point) ---
    context.beginPath();
    context.moveTo(toX, toY); // Tip
    context.lineTo(
        toX - headSize * Math.cos(angle - Math.PI / 6),
        toY - headSize * Math.sin(angle - Math.PI / 6)
    );
    context.lineTo(
        toX - headSize * Math.cos(angle + Math.PI / 6),
        toY - headSize * Math.sin(angle + Math.PI / 6)
    );
    context.closePath();
    context.fill(); // Fill the arrowhead

    // --- Draw the reverse arrowhead (if requested) ---
    if (hasReverseArrow) {
        context.beginPath();
        context.moveTo(fromX, fromY); // Tip of reverse arrow
        context.lineTo(
            fromX + headSize * Math.cos(angle - Math.PI / 6),
            fromY + headSize * Math.sin(angle - Math.PI / 6)
        );
        context.lineTo(
            fromX + headSize * Math.cos(angle + Math.PI / 6),
            fromY + headSize * Math.sin(angle + Math.PI / 6)
        );
        context.closePath();
        context.fill(); // Fill the arrowhead
    }

    context.restore(); // Restore context state
}