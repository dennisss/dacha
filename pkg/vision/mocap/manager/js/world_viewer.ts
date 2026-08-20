import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

// Maximum FPS at which to render. This is to keep the CPU usage
// of the animation well under control.
const MAX_FPS = 30;

const RAY_THICKNESS = 1; // 4 if you want it to be really visible.


function generateRigidBodyTriangulation(points) {
    const indices = [];
    if (points.length < 2) return indices;
    if (points.length === 2) return [0, 1];
    if (points.length === 3) return [0, 1, 1, 2, 2, 0];

    const vecs = points.map(pt => new THREE.Vector3(...pt.values));
    const n = vecs.length;
    const edgeSet = new Set();
    const degrees = new Array(n).fill(0);

    const addEdge = (i, j) => {
        const k = i < j ? `${i}-${j}` : `${j}-${i}`;
        if (!edgeSet.has(k)) {
            edgeSet.add(k);
            indices.push(i, j);
            degrees[i]++;
            degrees[j]++;
        }
    };

    // 1. Convex Hull Perimeter Edges (Angular span check around each axis)
    for (let i = 0; i < n; i++) {
        for (let j = i + 1; j < n; j++) {
            const u = new THREE.Vector3().subVectors(vecs[j], vecs[i]);
            const len = u.length();
            if (len < 1e-6) continue;
            u.normalize();

            // Construct simple 2D coordinate system on plane perpendicular to u
            const w = Math.abs(u.x) < 0.9 ? new THREE.Vector3(1, 0, 0) : new THREE.Vector3(0, 1, 0);
            const x = new THREE.Vector3().crossVectors(u, w).normalize();
            const y = new THREE.Vector3().crossVectors(u, x).normalize();

            const angles = [];
            let blockedByCollinear = false;

            for (let k = 0; k < n; k++) {
                if (k === i || k === j) continue;
                const v = new THREE.Vector3().subVectors(vecs[k], vecs[i]);
                const projX = v.dot(x);
                const projY = v.dot(y);
                const distSq = projX * projX + projY * projY;

                if (distSq < 1e-8) {
                    // Collinear point check: if point lies straight between i and j, break up the segment
                    const t = v.dot(u);
                    if (t > 1e-4 && t < len - 1e-4) {
                        blockedByCollinear = true;
                        break;
                    }
                } else {
                    angles.push(Math.atan2(projY, projX));
                }
            }

            if (blockedByCollinear) continue;

            if (angles.length <= 1) {
                addEdge(i, j);
            } else {
                angles.sort((a, b) => a - b);
                let maxGap = 0;
                for (let idx = 0; idx < angles.length - 1; idx++) {
                    maxGap = Math.max(maxGap, angles[idx + 1] - angles[idx]);
                }
                maxGap = Math.max(maxGap, (2 * Math.PI + angles[0]) - angles[angles.length - 1]);

                // If the largest empty angular gap around the line exceeds 180 deg (+ small tolerance),
                // all other points lie strictly on one side of a dividing plane -> guaranteed Convex Hull edge!
                if (maxGap > Math.PI + 0.005) {
                    addEdge(i, j);
                }
            }
        }
    }

    // 2. Gabriel Graph Edges (Connects local/interior structures without crossing diagonal noise)
    for (let i = 0; i < n; i++) {
        for (let j = i + 1; j < n; j++) {
            const mid = new THREE.Vector3().addVectors(vecs[i], vecs[j]).multiplyScalar(0.5);
            const radiusSq = vecs[i].distanceToSquared(vecs[j]) * 0.25;
            let isGabriel = true;
            for (let k = 0; k < n; k++) {
                if (k === i || k === j) continue;
                if (vecs[k].distanceToSquared(mid) < radiusSq - 1e-6) {
                    isGabriel = false;
                    break;
                }
            }
            if (isGabriel) {
                addEdge(i, j);
            }
        }
    }

    // 3. Connectivity safety check: Ensure every marker is connected to at least 2 neighbors
    for (let i = 0; i < n; i++) {
        if (degrees[i] < 2) {
            const dists = [];
            for (let j = 0; j < n; j++) {
                if (i !== j) {
                    dists.push({ index: j, dist: vecs[i].distanceToSquared(vecs[j]) });
                }
            }
            dists.sort((a, b) => a.dist - b.dist);
            for (let k = 0; k < dists.length && degrees[i] < 2; k++) {
                addEdge(i, dists[k].index);
            }
        }
    }

    return indices;
}

function createTextSprite(text, color) {
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const context = canvas.getContext('2d');
    context.font = 'bold 40px sans-serif';
    context.fillStyle = color;
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText(text, 32, 32);

    const texture = new THREE.CanvasTexture(canvas);
    texture.minFilter = THREE.LinearFilter;
    const spriteMaterial = new THREE.SpriteMaterial({ map: texture, depthTest: false, transparent: true });
    const sprite = new THREE.Sprite(spriteMaterial);
    sprite.scale.set(0.5, 0.5, 0.5);
    return sprite;
}

export class MocapWorldViewer {
    constructor(container, selectionBox) {
        this._container = container;
        this._selection_box_element = selectionBox;

        // Z-Up (East-North-Up)
        THREE.Object3D.DEFAULT_UP.set(0, 0, 1);

        this._scene = new THREE.Scene();
        this._scene.background = new THREE.Color('#f8f9fa');

        this._camera = new THREE.PerspectiveCamera(50, this._container.clientWidth / this._container.clientHeight, 0.1, 1000);
        this._camera.position.set(5, -5, 5);
        this._camera.lookAt(0, 0, 0);

        this._renderer = new THREE.WebGLRenderer({ antialias: true });
        this._renderer.setSize(this._container.clientWidth, this._container.clientHeight);
        this._renderer.setPixelRatio(window.devicePixelRatio);
        this._container.appendChild(this._renderer.domElement);

        // Add lighting for VRM materials
        const ambientLight = new THREE.AmbientLight(0xffffff, 1.0);
        this._scene.add(ambientLight);
        const directionalLight = new THREE.DirectionalLight(0xffffff, 2.0);
        directionalLight.position.set(1, 1, 1).normalize();
        this._scene.add(directionalLight);

        // Grid on XY plane
        this._gridHelper = this._createCustomGrid(0xe0e0e0);
        this._scene.add(this._gridHelper);

        // Axes (Red=X/East, Green=Y/North, Blue=Z/Up)
        const axesHelper = new THREE.AxesHelper(2);

        this._labels = [];
        const xLabel = createTextSprite('X', '#ff0000');
        xLabel.position.set(2.2, 0, 0);
        axesHelper.add(xLabel);
        this._labels.push(xLabel);

        const yLabel = createTextSprite('Y', '#00ff00');
        yLabel.position.set(0, 2.2, 0);
        axesHelper.add(yLabel);
        this._labels.push(yLabel);

        const zLabel = createTextSprite('Z', '#0000ff');
        zLabel.position.set(0, 0, 2.2);
        axesHelper.add(zLabel);
        this._labels.push(zLabel);

        this._scene.add(axesHelper);

        // Controls
        this._controls = new OrbitControls(this._camera, this._renderer.domElement);
        this._controls.enableDamping = false;
        this._controls.mouseButtons = {
            LEFT: null, // Free up left mouse for selection
            MIDDLE: THREE.MOUSE.PAN,
            RIGHT: THREE.MOUSE.ROTATE
        };

        // Data storage
        this._points_data = new Map();
        this._cameras_data = new Map();

        // Object storage
        this._point_meshes = new Map(); // id -> THREE.Mesh
        this._camera_groups = new Map(); // id -> THREE.Group
        this._rigid_body_groups = new Map(); // id -> THREE.Group
        this._skeleton_groups = new Map(); // id -> THREE.Group
        this._link_lines = []; // Array of THREE.Line
        this._camera_rays = []; // Array of THREE.Line
        this._camerasVisible = true;
        this._rigidBodiesVisible = false;
        this._rigidBodyAxesVisible = false;
        this._rigidBodyModelsVisible = false;
        this._rigid_body_template_model = null;
        this._skeletonVisible = false;
        this._skeletonMarkersVisible = false;

        // Materials
        this._point_material_high = new THREE.MeshBasicMaterial({ color: 0x0078D7 });
        this._point_material_low = new THREE.MeshBasicMaterial({ color: 0x0078D7, transparent: true, opacity: 0.6 });
        this._point_material_none = new THREE.MeshBasicMaterial({ color: 0x888888, transparent: true, opacity: 0.6 });
        this._selected_point_material = new THREE.MeshBasicMaterial({ color: 0xFF3B30 });
        this._camera_material = new THREE.LineBasicMaterial({ color: 0x34C759 });
        this._selected_camera_material = new THREE.LineBasicMaterial({ color: 0xFF9500 });
        this._link_material = new THREE.LineBasicMaterial({ color: 0x8E8E93 });
        this._ray_material = new THREE.LineDashedMaterial({ color: 0xFF3B30, dashSize: 0.1, gapSize: 0.1, linewidth: RAY_THICKNESS });

        this._rigid_body_material = new THREE.MeshBasicMaterial({ color: 0xff00ff, depthTest: false }); // Magenta sphere
        this._rigid_body_line_material = new THREE.LineBasicMaterial({ color: 0x00ffff, depthTest: false }); // Cyan lines
        this._skeleton_marker_material = new THREE.MeshStandardMaterial({ color: 0x00e676, transparent: true, opacity: 0.65, roughness: 0.4 }); // Semi-transparent green spheres

        // Selection state
        this._selected_point_ids = new Set();
        this._selected_camera_id = null;
        this.onSelectionChanged = null;

        // Interaction setup
        this._raycaster = new THREE.Raycaster();
        this._setup_selection();

        window.addEventListener('resize', this.onResize);

        this._running = false;
    }

    // Call this to start animating.
    start() {
        if (this._running) {
            return;
        }

        this._running = true;

        let clock = new THREE.Clock();
        let delta = 0;
        let interval = 1 / MAX_FPS;

        let animate = () => {
            if (!this._running) {
                return;
            }

            requestAnimationFrame(animate);

            let d = clock.getDelta();

            delta += d;
            if (delta > interval) {
                this._controls.update();
                this._renderer.render(this._scene, this._camera);
                delta %= interval;
            }
        };
        animate();
    }

    stop() {
        this._running = false;
    }

    setRigidBodiesVisible(visible) {
        this._rigidBodiesVisible = visible;
        this._rigid_body_groups.forEach(group => {
            group.children.forEach(child => {
                if (child.userData.isCentroid || child.userData.isTriangulation) {
                    child.visible = visible;
                }
            });
        });
    }

    setRigidBodyAxesVisible(visible) {
        this._rigidBodyAxesVisible = visible;
        this._rigid_body_groups.forEach(group => {
            group.children.forEach(child => {
                if (child.userData.isAxes) {
                    child.visible = visible;
                }
            });
        });
    }

    setRigidBodyModelsVisible(visible) {
        this._rigidBodyModelsVisible = visible;
        this._rigid_body_groups.forEach(group => {
            group.children.forEach(child => {
                if (child.userData.isModelContainer) {
                    child.visible = visible;
                }
            });
        });
    }

    setSkeletonVisible(visible) {
        this._skeletonVisible = visible;
        this._skeleton_groups.forEach(group => {
            if (group.userData.bonesAndJointsGroup) {
                group.userData.bonesAndJointsGroup.visible = visible;
            } else {
                group.visible = visible;
            }
        });
    }

    setSkeletonMarkersVisible(visible) {
        this._skeletonMarkersVisible = visible;
        this._skeleton_groups.forEach(group => {
            if (group.userData.markersGroup) {
                group.userData.markersGroup.visible = visible;
            }
        });
    }

    setLabelsVisible(visible) {
        this._labels.forEach(label => label.visible = visible);
    }

    setCamerasVisible(visible) {
        this._camerasVisible = visible;
        this._camera_groups.forEach(group => group.visible = visible);
    }

    _createCustomGrid(colorHex) {
        const size = 10;
        const divisions = 10;
        const halfSize = size / 2;
        const step = size / divisions;

        const vertices = [];
        for (let i = -halfSize; i <= halfSize; i += step) {
            if (Math.abs(i) > 0.001) {
                // Normal grid lines
                // Line parallel to Y
                vertices.push(i, -halfSize, 0, i, halfSize, 0);
                // Line parallel to X
                vertices.push(-halfSize, i, 0, halfSize, i, 0);
            } else {
                // Central axes: AxesHelper draws the positive halves (0 to 2),
                // so we draw the negative halves (-halfSize to 0) and the
                // remaining positive halves (2 to halfSize) to avoid z-fighting.

                // Line along Y axis (X=0)
                vertices.push(0, -halfSize, 0, 0, 0, 0); // Negative
                vertices.push(0, 2, 0, 0, halfSize, 0);  // Positive remainder

                // Line along X axis (Y=0)
                vertices.push(-halfSize, 0, 0, 0, 0, 0); // Negative
                vertices.push(2, 0, 0, halfSize, 0, 0);  // Positive remainder
            }
        }
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.Float32BufferAttribute(vertices, 3));
        const material = new THREE.LineBasicMaterial({ color: colorHex, depthWrite: false });
        const grid = new THREE.LineSegments(geometry, material);
        return grid;
    }

    setDarkMode(isDark) {
        if (isDark) {
            this._scene.background = new THREE.Color('#121212');
            this._scene.remove(this._gridHelper);
            this._gridHelper = this._createCustomGrid(0x222222);
            this._scene.add(this._gridHelper);
        } else {
            this._scene.background = new THREE.Color('#f8f9fa');
            this._scene.remove(this._gridHelper);
            this._gridHelper = this._createCustomGrid(0xe0e0e0);
            this._scene.add(this._gridHelper);
        }
    }

    update(data) {
        {
            const seenCameras = new Set();
            (data.cameras || []).forEach(cam => {
                seenCameras.add(cam.id);
                this._cameras_data.set(cam.id, cam);
                if (!this._camera_groups.has(cam.id)) {
                    this._create_camera_object(cam);
                } else {
                    this._update_camera_object(cam);
                }
            });

            // Remove unseen cameras
            for (const [id, group] of this._camera_groups) {
                if (!seenCameras.has(id)) {
                    this._scene.remove(group);
                    this._camera_groups.delete(id);
                    this._cameras_data.delete(id);
                    if (this._selected_camera_id === id) this._selected_camera_id = null;
                }
            }
        }

        {
            const seenPoints = new Set();
            (data.points || []).forEach(pt => {
                seenPoints.add(pt.id);
                this._points_data.set(pt.id, pt);
                if (!this._point_meshes.has(pt.id)) {
                    this._create_point_object(pt);
                } else {
                    this._update_point_object(pt);
                }
            });

            // Remove unseen points
            for (const [id, mesh] of this._point_meshes) {
                if (!seenPoints.has(id)) {
                    this._scene.remove(mesh);
                    this._point_meshes.delete(id);
                    this._points_data.delete(id);
                    this._selected_point_ids.delete(id);
                }
            }
        }

        {
            const seenRigidBodies = new Set();
            (data.rigid_bodies || []).forEach(rb => {
                seenRigidBodies.add(rb.id);
                if (!this._rigid_body_groups.has(rb.id)) {
                    this._create_rigid_body_object(rb);
                }
                this._update_rigid_body_object(rb);
            });

            // Remove unseen rigid bodies
            for (const [id, group] of this._rigid_body_groups) {
                if (!seenRigidBodies.has(id)) {
                    this._scene.remove(group);
                    this._rigid_body_groups.delete(id);
                }
            }
        }

        {
            const seenSkeletons = new Set();
            (data.skeletons || []).forEach(skel => {
                seenSkeletons.add(skel.id);
                if (!this._skeleton_groups.has(skel.id)) {
                    this._create_skeleton_object(skel);
                }
                this._update_skeleton_object(skel);
            });

            // Remove unseen skeletons
            for (const [id, group] of this._skeleton_groups) {
                if (!seenSkeletons.has(id)) {
                    this._scene.remove(group);
                    group.userData.bonePool.forEach(b => { b.geometry.dispose(); if (b.material.dispose) b.material.dispose(); });
                    group.userData.jointPool.forEach(j => { j.geometry.dispose(); if (j.material.dispose) j.material.dispose(); });
                    if (group.userData.markerPool) group.userData.markerPool.forEach(m => { m.geometry.dispose(); if (m.material.dispose) m.material.dispose(); });
                    this._skeleton_groups.delete(id);
                }
            }
        }

        if (data.links) {
            // Rebuild links
            this._link_lines.forEach(line => {
                this._scene.remove(line);
                line.geometry.dispose(); // Good practice to avoid memory leaks since we recreate them
            });
            this._link_lines = [];
            data.links.forEach(link => {
                const pt1 = this._points_data.get(link.point1_id);
                const pt2 = this._points_data.get(link.point2_id);
                if (pt1 && pt2) {
                    const geometry = new THREE.BufferGeometry().setFromPoints([
                        new THREE.Vector3(...pt1.position.values),
                        new THREE.Vector3(...pt2.position.values)
                    ]);
                    const line = new THREE.Line(geometry, this._link_material);
                    this._scene.add(line);
                    this._link_lines.push(line);
                }
            });
        }

        this._update_rays();
    }

    _create_rigid_body_object(rb) {
        const group = new THREE.Group();
        group.visible = true; // Always keep root group visible so independent child overlays work

        // 1. Sphere at centroid
        const sphereGeo = new THREE.SphereGeometry(0.02, 16, 16);
        const sphereMesh = new THREE.Mesh(sphereGeo, this._rigid_body_material);
        sphereMesh.renderOrder = 999; // Render on top
        sphereMesh.userData.isCentroid = true;
        sphereMesh.visible = this._rigidBodiesVisible;
        group.add(sphereMesh);

        // 2. Triangulation lines
        if (rb.points && rb.points.length > 1) {
            group.userData.pointsCacheKey = JSON.stringify(rb.points);
            const lines = this._create_rigid_body_lines(rb.points);
            lines.userData.isTriangulation = true;
            lines.visible = this._rigidBodiesVisible;
            group.add(lines);
            group.userData.linesMesh = lines;
        }

        // 3. Axes Helper
        const axes = new THREE.AxesHelper(0.1);
        axes.userData.isAxes = true;
        axes.visible = this._rigidBodyAxesVisible;
        group.add(axes);

        // 4. 3D Model Overlay Container
        const modelContainer = new THREE.Group();
        modelContainer.userData.isModelContainer = true;
        modelContainer.visible = this._rigidBodyModelsVisible;
        if (this._rigid_body_template_model) {
            modelContainer.add(this._rigid_body_template_model.clone());
        }
        group.add(modelContainer);

        this._scene.add(group);
        this._rigid_body_groups.set(rb.id, group);
    }

    _update_rigid_body_object(rb) {
        const group = this._rigid_body_groups.get(rb.id);

        // Translation
        if (rb.translation && rb.translation.values) {
            group.position.set(...rb.translation.values);
        }

        // Rotation (Rodrigues vector to Quaternion)
        if (rb.rotation && rb.rotation.values) {
            const r = rb.rotation.values;
            const rx = r[0], ry = r[1], rz = r[2];
            const angle = Math.sqrt(rx * rx + ry * ry + rz * rz);
            if (angle > 0.0001) {
                const axis = new THREE.Vector3(rx / angle, ry / angle, rz / angle);
                console.log(axis, angle);
                group.quaternion.setFromAxisAngle(axis, angle);
            } else {
                group.quaternion.identity();
            }
        }

        // Triangulation update
        if (rb.points && rb.points.length > 1) {
            const cacheKey = JSON.stringify(rb.points);
            if (cacheKey !== group.userData.pointsCacheKey) {
                group.userData.pointsCacheKey = cacheKey;

                if (group.userData.linesMesh) {
                    group.remove(group.userData.linesMesh);
                    group.userData.linesMesh.geometry.dispose();
                }

                const lines = this._create_rigid_body_lines(rb.points);
                lines.userData.isTriangulation = true;
                lines.visible = this._rigidBodiesVisible;
                group.add(lines);
                group.userData.linesMesh = lines;
            }
        } else if (group.userData.linesMesh) {
            group.remove(group.userData.linesMesh);
            group.userData.linesMesh.geometry.dispose();
            group.userData.linesMesh = null;
            group.userData.pointsCacheKey = null;
        }
    }

    _create_skeleton_object(skel) {
        const group = new THREE.Group();
        group.visible = true; // Root group stays visible so sub-groups control independently

        const bonesAndJointsGroup = new THREE.Group();
        bonesAndJointsGroup.visible = this._skeletonVisible;
        group.add(bonesAndJointsGroup);

        const markersGroup = new THREE.Group();
        markersGroup.visible = this._skeletonMarkersVisible;
        group.add(markersGroup);

        group.userData.bonePool = [];
        group.userData.jointPool = [];
        group.userData.markerPool = [];
        group.userData.bonesAndJointsGroup = bonesAndJointsGroup;
        group.userData.markersGroup = markersGroup;

        this._scene.add(group);
        this._skeleton_groups.set(skel.id, group);
    }

    _update_skeleton_object(skel) {
        const group = this._skeleton_groups.get(skel.id);
        if (!group) return;

        const bonePool = group.userData.bonePool;
        const jointPool = group.userData.jointPool;
        const markerPool = group.userData.markerPool || [];
        const bonesAndJointsGroup = group.userData.bonesAndJointsGroup || group;
        const markersGroup = group.userData.markersGroup || group;

        // Hide all existing
        bonePool.forEach(b => b.visible = false);
        jointPool.forEach(j => j.visible = false);
        markerPool.forEach(m => m.visible = false);

        if (skel.start && skel.end && skel.start.length === skel.end.length) {
            const numBones = skel.start.length;
            const jointData = [];
            const addJointData = (pos, colorHex) => {
                for (let i = 0; i < jointData.length; i++) {
                    const dx = jointData[i].pos[0] - pos[0];
                    const dy = jointData[i].pos[1] - pos[1];
                    const dz = jointData[i].pos[2] - pos[2];
                    if (dx * dx + dy * dy + dz * dz < 0.000001) {
                        // If existing joint is white (neutral) and the new one is colored (Left/Right), override with colored
                        if (jointData[i].colorHex === 0xffffff && colorHex !== 0xffffff) {
                            jointData[i].colorHex = colorHex;
                        }
                        return;
                    }
                }
                jointData.push({ pos, colorHex });
            };

            const vecStart = new THREE.Vector3();
            const vecEnd = new THREE.Vector3();

            for (let i = 0; i < numBones; i++) {
                const s = skel.start[i].values;
                const e = skel.end[i].values;

                let boneColorHex = 0xaaaaaa;
                let jointColorHex = 0xffffff;
                if (skel.bone_names && skel.bone_names[i]) {
                    const name = skel.bone_names[i].toUpperCase();
                    if (name.includes('LEFT')) {
                        boneColorHex = 0xff4444;
                        jointColorHex = 0xff4444;
                    } else if (name.includes('RIGHT')) {
                        boneColorHex = 0x4444ff;
                        jointColorHex = 0x4444ff;
                    }
                }

                vecStart.set(s[0], s[1], s[2]);
                vecEnd.set(e[0], e[1], e[2]);

                const distance = vecStart.distanceTo(vecEnd);
                if (distance > 0.0001) {
                    let bone;
                    if (i < bonePool.length) {
                        bone = bonePool[i];
                    } else {
                        const geom = new THREE.CylinderGeometry(0.015, 0.015, 1, 8);
                        geom.translate(0, 0.5, 0); // Pivot at base
                        geom.rotateX(Math.PI / 2); // Point along Z axis
                        const mat = new THREE.MeshStandardMaterial({ roughness: 0.7 });
                        bone = new THREE.Mesh(geom, mat);
                        bonesAndJointsGroup.add(bone);
                        bonePool.push(bone);
                    }

                    bone.material.color.setHex(boneColorHex);
                    bone.position.copy(vecStart);
                    bone.scale.set(1, 1, distance);
                    bone.lookAt(vecEnd);
                    bone.visible = true;
                }

                addJointData(s, jointColorHex);
                addJointData(e, jointColorHex);
            }

            for (let i = 0; i < jointData.length; i++) {
                const { pos, colorHex } = jointData[i];
                let joint;
                if (i < jointPool.length) {
                    joint = jointPool[i];
                } else {
                    const geom = new THREE.SphereGeometry(0.025, 16, 16);
                    const mat = new THREE.MeshStandardMaterial({ roughness: 0.3 });
                    joint = new THREE.Mesh(geom, mat);
                    bonesAndJointsGroup.add(joint);
                    jointPool.push(joint);
                }
                joint.material.color.setHex(colorHex);
                joint.material.emissive.setHex(colorHex).multiplyScalar(0.5);
                joint.position.set(...pos);
                joint.visible = true;
            }
        }

        if (skel.markers && skel.markers.length > 0) {
            let markerIndex = 0;
            for (let i = 0; i < skel.markers.length; i++) {
                const posObj = skel.markers[i];
                if (!posObj || !posObj.values) continue;
                let marker;
                if (markerIndex < markerPool.length) {
                    marker = markerPool[markerIndex];
                } else {
                    const geom = new THREE.SphereGeometry(0.04, 16, 16);
                    marker = new THREE.Mesh(geom, this._skeleton_marker_material);
                    markersGroup.add(marker);
                    markerPool.push(marker);
                }
                marker.position.set(...posObj.values);
                marker.visible = true;
                markerIndex++;
            }
            group.userData.markerPool = markerPool;
        }
    }

    _create_rigid_body_lines(points) {
        const positions = new Float32Array(points.length * 3);
        points.forEach((pt, i) => {
            positions[i * 3] = pt.values[0];
            positions[i * 3 + 1] = pt.values[1];
            positions[i * 3 + 2] = pt.values[2];
        });

        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

        const indices = generateRigidBodyTriangulation(points);
        geometry.setIndex(indices);

        const lines = new THREE.LineSegments(geometry, this._rigid_body_line_material);
        lines.renderOrder = 998;
        return lines;
    }

    _create_camera_object(cam) {
        const group = new THREE.Group();

        // Camera body (box)
        const boxGeo = new THREE.BoxGeometry(0.2, 0.2, 0.2);
        const boxEdges = new THREE.EdgesGeometry(boxGeo);
        const boxLine = new THREE.LineSegments(boxEdges, this._camera_material);
        group.add(boxLine);

        // Frustum pyramid to show direction
        const frustumGeo = new THREE.BufferGeometry();
        const vertices = new Float32Array([
            0, 0, 0, 0.2, 0.2, -0.4,
            0, 0, 0, -0.2, 0.2, -0.4,
            0, 0, 0, 0.2, -0.2, -0.4,
            0, 0, 0, -0.2, -0.2, -0.4,
            0.2, 0.2, -0.4, -0.2, 0.2, -0.4,
            -0.2, 0.2, -0.4, -0.2, -0.2, -0.4,
            -0.2, -0.2, -0.4, 0.2, -0.2, -0.4,
            0.2, -0.2, -0.4, 0.2, 0.2, -0.4
        ]);
        frustumGeo.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
        const frustumLine = new THREE.LineSegments(frustumGeo, this._camera_material);
        group.add(frustumLine);

        group.userData = { id: cam.id, type: 'camera' };

        // To make Raycaster work with lines, we can add a transparent mesh
        const hitBoxGeo = new THREE.BoxGeometry(0.4, 0.4, 0.6);
        hitBoxGeo.translate(0, 0, -0.2);
        const hitBoxMat = new THREE.MeshBasicMaterial({ visible: false });
        const hitBox = new THREE.Mesh(hitBoxGeo, hitBoxMat);
        hitBox.userData = { id: cam.id, type: 'camera' };
        group.add(hitBox);

        group.visible = this._camerasVisible;

        this._scene.add(group);
        this._camera_groups.set(cam.id, group);
        this._update_camera_object(cam);
    }

    _update_camera_object(cam) {
        const group = this._camera_groups.get(cam.id);

        // rotation is World-to-Camera in axis-angle format [rx, ry, rz]
        const r = cam.rotation;
        const rx = r[0], ry = r[1], rz = r[2];
        const angle = Math.sqrt(rx * rx + ry * ry + rz * rz);

        const qW2C = new THREE.Quaternion();
        if (angle > 0.0001) {
            const axis = new THREE.Vector3(rx / angle, ry / angle, rz / angle);
            qW2C.setFromAxisAngle(axis, angle);
        }

        // Camera-to-World rotation is the inverse
        const qC2W = qW2C.clone().invert();

        // translation is World-to-Camera t_w2c
        // Camera-to-World position is -R_c2w * t_w2c
        const tW2C = new THREE.Vector3(...cam.translation);
        const tC2W = tW2C.clone().applyQuaternion(qC2W).negate();
        group.position.copy(tC2W);

        // Convert from OpenCV (Z forward, Y down) to Three.js (Z backward, Y up)
        // Negating the Y and Z axes is mathematically equivalent to applying a 
        // 180-degree rotation around the local X axis.
        const qFix = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), Math.PI);

        group.quaternion.multiplyQuaternions(qC2W, qFix);

        // Update selection color
        const material = this._selected_camera_id === cam.id ? this._selected_camera_material : this._camera_material;
        group.children[0].material = material;
        group.children[1].material = material;
    }

    _get_point_material(pt) {
        if (this._selected_point_ids.has(pt.id)) {
            return this._selected_point_material;
        }
        if (!pt.camera_ids || pt.camera_ids.length === 0) {
            return this._point_material_none;
        }
        if (pt.camera_ids.length >= 3) {
            return this._point_material_high;
        }
        return this._point_material_low;
    }

    _create_point_object(pt) {
        const geometry = new THREE.SphereGeometry(pt.radius || 0.05, 16, 16);
        const mesh = new THREE.Mesh(geometry, this._get_point_material(pt));
        mesh.userData = { id: pt.id, type: 'point' };
        this._scene.add(mesh);
        this._point_meshes.set(pt.id, mesh);
        this._update_point_object(pt);
    }

    _update_point_object(pt) {
        const mesh = this._point_meshes.get(pt.id);
        mesh.position.set(...pt.position.values);
        mesh.scale.setScalar(1); // Could scale based on radius if it changes

        mesh.material = this._get_point_material(pt);
    }

    _setup_selection() {
        let isDragging = false;
        let startX = 0;
        let startY = 0;

        const getLocalPos = (e) => {
            const rect = this._container.getBoundingClientRect();
            return {
                x: e.clientX - rect.left,
                y: e.clientY - rect.top
            };
        };

        this._container.addEventListener('pointerdown', (e) => {
            if (e.button !== 0) return; // Only left click
            isDragging = true;
            const pos = getLocalPos(e);
            startX = pos.x;
            startY = pos.y;

            this._selection_box_element.style.left = startX + 'px';
            this._selection_box_element.style.top = startY + 'px';
            this._selection_box_element.style.width = '0px';
            this._selection_box_element.style.height = '0px';
            this._selection_box_element.classList.remove('hidden');
        });

        this._container.addEventListener('pointermove', (e) => {
            if (!isDragging) return;

            const pos = getLocalPos(e);
            const currentX = pos.x;
            const currentY = pos.y;

            const left = Math.min(startX, currentX);
            const top = Math.min(startY, currentY);
            const width = Math.abs(currentX - startX);
            const height = Math.abs(currentY - startY);

            this._selection_box_element.style.left = left + 'px';
            this._selection_box_element.style.top = top + 'px';
            this._selection_box_element.style.width = width + 'px';
            this._selection_box_element.style.height = height + 'px';
        });

        this._container.addEventListener('pointerup', (e) => {
            if (e.button !== 0 || !isDragging) return;
            isDragging = false;
            this._selection_box_element.classList.add('hidden');

            const pos = getLocalPos(e);
            const currentX = pos.x;
            const currentY = pos.y;

            if (Math.abs(currentX - startX) < 3 && Math.abs(currentY - startY) < 3) {
                // Single click
                this._handle_single_click(currentX, currentY);
            } else {
                // Box selection
                this._handle_box_selection(
                    Math.min(startX, currentX), Math.min(startY, currentY),
                    Math.max(startX, currentX), Math.max(startY, currentY)
                );
            }
        });
    }

    _handle_single_click(localX, localY) {
        const rect = this._container.getBoundingClientRect();
        const mouse = new THREE.Vector2(
            (localX / rect.width) * 2 - 1,
            -(localY / rect.height) * 2 + 1
        );

        this._raycaster.setFromCamera(mouse, this._camera);

        const objectsToTest = [];
        this._point_meshes.forEach(mesh => objectsToTest.push(mesh));
        this._camera_groups.forEach(group => objectsToTest.push(group.children[2])); // The hitBox

        const intersects = this._raycaster.intersectObjects(objectsToTest);

        this._selected_point_ids.clear();
        this._selected_camera_id = null;

        if (intersects.length > 0) {
            const userData = intersects[0].object.userData;
            if (userData.type === 'point') {
                this._selected_point_ids.add(userData.id);
            } else if (userData.type === 'camera') {
                this._selected_camera_id = userData.id;
            }
        }

        this._refresh_selection_state();
    }

    _handle_box_selection(minX, minY, maxX, maxY) {
        this._selected_point_ids.clear();
        this._selected_camera_id = null;

        const rect = this._container.getBoundingClientRect();

        this._point_meshes.forEach((mesh, id) => {
            const pos = mesh.position.clone().project(this._camera);
            // Check if behind camera
            if (pos.z > 1) return;

            const sx = (pos.x + 1) / 2 * rect.width;
            const sy = (-pos.y + 1) / 2 * rect.height;

            if (sx >= minX && sx <= maxX && sy >= minY && sy <= maxY) {
                this._selected_point_ids.add(id);
            }
        });

        this._refresh_selection_state();
    }

    _refresh_selection_state() {
        // Update visuals
        this._point_meshes.forEach((mesh, id) => {
            const pt = this._points_data.get(id);
            if (pt) {
                mesh.material = this._get_point_material(pt);
            }
        });

        this._camera_groups.forEach((group, id) => {
            const material = this._selected_camera_id === id ? this._selected_camera_material : this._camera_material;
            group.children[0].material = material;
            group.children[1].material = material;
        });

        this._update_rays();

        // Fire callback
        if (this.onSelectionChanged) {
            this.onSelectionChanged(Array.from(this._selected_point_ids));
        }
    }

    _update_rays() {
        // Clear old rays
        this._camera_rays.forEach(ray => this._scene.remove(ray));
        this._camera_rays = [];

        // Draw rays if exactly ONE point is selected
        if (this._selected_point_ids.size === 1) {
            const pointId = Array.from(this._selected_point_ids)[0];
            const pt = this._points_data.get(pointId);

            if (pt && pt.camera_ids) {
                pt.camera_ids.forEach(camId => {
                    const camGroup = this._camera_groups.get(camId);
                    if (camGroup) {
                        const geometry = new THREE.BufferGeometry().setFromPoints([
                            camGroup.position.clone(),
                            new THREE.Vector3(...pt.position.values)
                        ]);
                        const line = new THREE.Line(geometry, this._ray_material);
                        line.computeLineDistances(); // Required for dashed lines
                        this._scene.add(line);
                        this._camera_rays.push(line);
                    }
                });
            }
        }
    }

    onResize = () => {
        this._camera.aspect = this._container.clientWidth / this._container.clientHeight;
        this._camera.updateProjectionMatrix();
        this._renderer.setSize(this._container.clientWidth, this._container.clientHeight);
    }
}
