// This is all code written by Google Gemini

/**
 * Class to define and draw a realistic involute gear.
 * The geometry is based on standard gear engineering parameters.
 */
export class Gear {
    constructor(options) {
        // --- Core Gear Parameters ---
        this.x = options.x; // Center x-coordinate
        this.y = options.y; // Center y-coordinate
        this.numTeeth = options.numTeeth || 20;

        // Module is a standard gear parameter: (Pitch Diameter / Number of Teeth)
        this.module = options.module || 10;

        // Pressure angle (phi) is typically 20 degrees for standard gears
        this.pressureAngle = (options.pressureAngle || 20) * (Math.PI / 180);

        this.fillColor = options.fillColor || '#606060';
        this.strokeColor = options.strokeColor || '#404040';
        this.strokeWidth = options.strokeWidth || 2;

        // --- Calculated Geometric Properties ---

        // Pitch Radius: The radius of the "working" circle of the gear
        this.pitchRadius = (this.numTeeth * this.module) / 2;

        // Base Radius: The radius from which the involute curve is generated
        this.baseRadius = this.pitchRadius * Math.cos(this.pressureAngle);

        // Addendum: The height of the tooth above the pitch circle
        this.addendum = this.module;

        // Dedendum: The depth of the tooth below the pitch circle (1.25 is a common standard)
        this.dedendum = this.module * 1.25;

        // Outer Radius (to the tip of the tooth)
        this.outerRadius = this.pitchRadius + this.addendum;

        // Root Radius (to the bottom of the tooth gap)
        this.rootRadius = this.pitchRadius - this.dedendum;

        // Internal hole radius
        this.boreRadius = options.boreRadius || this.pitchRadius / 4;
    }

    /**
     * Generates the [x, y] coordinates for an involute curve point.
     * @param {number} baseRadius - The radius of the base circle.
     * @param {number} t - The roll angle (theta) of the curve.
     * @returns {Array<number>} [x, y] point
     */
    getInvolutePoint(baseRadius, t) {
        const x = baseRadius * (Math.cos(t) + t * Math.sin(t));
        const y = baseRadius * (Math.sin(t) - t * Math.cos(t));
        return [x, y];
    }

    /**
     * Draws the gear shape onto the canvas context.
     * @param {CanvasRenderingContext2D} ctx - The 2D rendering context of the canvas.
     */
    draw(ctx) {
        ctx.save();
        ctx.fillStyle = this.fillColor;
        ctx.strokeStyle = this.strokeColor;
        ctx.lineWidth = this.strokeWidth;

        // Move origin to the gear's center
        ctx.translate(this.x, this.y);

        ctx.beginPath();

        // Calculate the max "roll angle" t for the involute curve
        // This finds where the involute curve intersects the outer radius
        const tMax = Math.sqrt(Math.pow(this.outerRadius / this.baseRadius, 2) - 1);

        // Angle between the start of one tooth and the next
        const toothAngle = 2 * Math.PI / this.numTeeth;

        // Angle of the involute part of the tooth at the pitch circle
        const involuteAngleAtPitch = Math.tan(this.pressureAngle) - this.pressureAngle;

        for (let i = 0; i < this.numTeeth; i++) {
            const angle = i * toothAngle;

            // --- 1. Draw Root Circle Arc ---
            // We start at the root circle, between two teeth
            const startAngle = angle + (toothAngle / 4) + involuteAngleAtPitch;
            const endAngle = angle + (toothAngle * 3 / 4) - involuteAngleAtPitch;

            // Safety check: if root radius is less than base, draw straight line
            if (this.rootRadius > this.baseRadius) {
                ctx.arc(0, 0, this.rootRadius, startAngle, endAngle);
            } else {
                const [startX, startY] = [this.rootRadius * Math.cos(startAngle), this.rootRadius * Math.sin(startAngle)];
                const [endX, endY] = [this.rootRadius * Math.cos(endAngle), this.rootRadius * Math.sin(endAngle)];
                ctx.lineTo(startX, startY);
                ctx.lineTo(endX, endY);
            }

            // --- 2. Draw Rising Involute Curve (right side of tooth) ---
            // We draw from t=0 (at base circle) up to t=tMax (at outer circle)
            // We rotate the canvas to position the tooth correctly
            const risingSideAngle = angle + (toothAngle * 3 / 4) - involuteAngleAtPitch;

            ctx.save();
            ctx.rotate(risingSideAngle);
            for (let t = 0; t <= tMax; t += 0.1) { // Draw curve in small steps
                const [x, y] = this.getInvolutePoint(this.baseRadius, t);
                ctx.lineTo(x, y);
            }
            ctx.restore();

            /*
            // --- 3. Draw Outer Circle Arc (tooth tip) ---
            const tipStartAngle = risingSideAngle + (tMax - Math.tan(tMax));
            const tipEndAngle = angle + (toothAngle * 5 / 4) + involuteAngleAtPitch - (tMax - Math.tan(tMax));
            ctx.arc(0, 0, this.outerRadius, tipStartAngle, tipEndAngle);
            */

            // --- 4. Draw Falling Involute Curve (left side of next tooth) ---
            const fallingSideAngle = angle + (toothAngle * 5 / 4) + involuteAngleAtPitch;
            ctx.save();
            ctx.rotate(fallingSideAngle);
            for (let t = tMax; t >= 0; t -= 0.1) { // Draw curve in reverse
                const [x, y] = this.getInvolutePoint(this.baseRadius, t);
                ctx.lineTo(x, -y); // Mirror the y-coordinate
            }
            ctx.restore();
        }

        ctx.closePath();
        ctx.fill();
        ctx.stroke();

        // --- 5. Draw Center Bore (Hole) ---
        ctx.beginPath();
        ctx.arc(0, 0, this.boreRadius, 0, 2 * Math.PI);
        ctx.fillStyle = '#f0f0f0'; // "Cut out" the hole
        ctx.fill();
        ctx.stroke(); // Add a stroke to the hole

        ctx.restore(); // Restore context to original state (no translation)
    }
}