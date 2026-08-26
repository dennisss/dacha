"""
This script generates gcode for my Makera Carvera to thread mill the tripod mount holes on the case 3d printed parts.

In the ATC (1/4" shaft):
- Tool 1: 20 TPI single tooth thread milling 
- Tool 2: 1/8" end mill

The operations are:
- Grab end mill
- Position over center of hole (1mm above)
- Bore out hole to 5.1mm
- Switch to thread mill
- Apply 2 passes of cutting the threads.

Notes:
- On the Carvera, you can use the following to reset the coordinate system to machine coordinates:
    - "G10 L2 P1 X0 Y0 Z0"

"""

import math

def generate_thread_milling_gcode(
    filename,
    center_x_mm=0.0,           
    center_y_mm=0.0,           
    center_z_mm=0.0,           
    hole_depth_mm=6.0,         # Absolute max depth for the bottom of the hole
    thread_clearance_mm=0.1,   # How far above the bottom the thread mill stops its plunge
    target_minor_dia_mm=5.1,   
    endmill_dia_mm=3.175,      
    tpi=20.0,
    major_dia_mm=6.35,         
    threadmill_dia_mm=4.5,     
    fit_offset_mm=0.1,
    safe_z_mm=5.0,             
    bore_pitch_mm=1.0,         
    feed_rate_mm=200.0,        
    plunge_rate_mm=100.0,      
    spindle_speed=10000        
):
    # --- Threading Calculations ---
    thread_pitch_mm = 25.4 / tpi
    
    adjusted_major_dia = major_dia_mm + fit_offset_mm
    
    if threadmill_dia_mm >= adjusted_major_dia:
        raise ValueError("Error: Thread cutter diameter must be smaller than the thread major diameter.")
    
    thread_path_dia = adjusted_major_dia - threadmill_dia_mm
    t_r = thread_path_dia / 2.0
    t_edge_x = center_x_mm + t_r
    
    # --- Boring Calculations ---
    if endmill_dia_mm >= target_minor_dia_mm:
        raise ValueError("Error: End mill must be smaller than the target minor diameter.")
        
    bore_path_dia = target_minor_dia_mm - endmill_dia_mm
    b_r = bore_path_dia / 2.0
    b_edge_x = center_x_mm + b_r

    # --- Z Heights ---
    safe_z_abs = center_z_mm + safe_z_mm
    inspection_z = center_z_mm + 1.0
    
    # End mill goes exactly to the max depth
    bore_target_z = center_z_mm - hole_depth_mm
    
    # Thread mill stays 0.1mm above the bored floor
    thread_start_z = bore_target_z + thread_clearance_mm
    
    thread_target_z = center_z_mm + thread_pitch_mm  

    gcode = []
    
    # --- Setup Block ---
    gcode.append("G21")
    gcode.append("G90")

    gcode.append("M6 T2")
    gcode.append(f"G0 Z-15")
    
    # ==========================================
    # PHASE 1: BORE HOLE TO MAX DEPTH
    # ==========================================
    gcode.append(f"G0 X{center_x_mm:.4f} Y{center_y_mm:.4f}")
    gcode.append(f"G0 Z{inspection_z:.4f}")
    # gcode.append("M0") 
    gcode.append(f"M3 S{spindle_speed}")
    
    gcode.append(f"G1 Z{center_z_mm:.4f} F{plunge_rate_mm}")
    gcode.append(f"G3 X{b_edge_x:.4f} Y{center_y_mm:.4f} I{b_r/2:.4f} J0.0000 F{feed_rate_mm}")
    
    current_z = center_z_mm
    # Added +0.001 to prevent floating point skipping
    while current_z > (bore_target_z + 0.001):
        current_z -= bore_pitch_mm
        if current_z < bore_target_z:
            current_z = bore_target_z 
        gcode.append(f"G3 X{b_edge_x:.4f} Y{center_y_mm:.4f} Z{current_z:.4f} I{-b_r:.4f} J0.0000")
        
    gcode.append(f"G3 X{b_edge_x:.4f} Y{center_y_mm:.4f} I{-b_r:.4f} J0.0000")
    gcode.append(f"G3 X{center_x_mm:.4f} Y{center_y_mm:.4f} I{-b_r/2:.4f} J0.0000")
    gcode.append(f"G0 Z{safe_z_abs:.4f}")
    gcode.append("M5")
    
    # ==========================================
    # PHASE 2: THREAD MILLING
    # ==========================================
    gcode.append("M6 T1")
    gcode.append(f"G0 X{center_x_mm:.4f} Y{center_y_mm:.4f}")
    gcode.append(f"G0 Z{inspection_z:.4f}")
    gcode.append(f"M3 S{spindle_speed}")

    # Plunges safely to thread_start_z (0.1mm above the floor)
    gcode.append(f"G1 Z{thread_start_z:.4f} F{plunge_rate_mm}")
    gcode.append(f"G3 X{t_edge_x:.4f} Y{center_y_mm:.4f} I{t_r/2:.4f} J0.0000 F{feed_rate_mm}")
    
    current_z = thread_start_z
    while current_z < (thread_target_z - 0.001):
        current_z += thread_pitch_mm
        gcode.append(f"G3 X{t_edge_x:.4f} Y{center_y_mm:.4f} Z{current_z:.4f} I{-t_r:.4f} J0.0000")
        
    gcode.append(f"G3 X{center_x_mm:.4f} Y{center_y_mm:.4f} I{-t_r/2:.4f} J0.0000")
    
    gcode.append(f"G0 Z{thread_start_z:.4f}") 
    gcode.append(f"G3 X{t_edge_x:.4f} Y{center_y_mm:.4f} I{t_r/2:.4f} J0.0000 F{feed_rate_mm}")
    
    current_z = thread_start_z
    while current_z < (thread_target_z - 0.001):
        current_z += thread_pitch_mm
        gcode.append(f"G3 X{t_edge_x:.4f} Y{center_y_mm:.4f} Z{current_z:.4f} I{-t_r:.4f} J0.0000")
        
    gcode.append(f"G3 X{center_x_mm:.4f} Y{center_y_mm:.4f} I{-t_r/2:.4f} J0.0000")
    gcode.append(f"G0 Z{safe_z_abs:.4f}")
    gcode.append("M5")
    gcode.append("M30")

    gcode.append(f"G0 Z-15")
    gcode.append(f"G0 X-25 Y-25")

    with open(filename, 'w') as f:
        f.write('\n'.join(gcode))

if __name__ == "__main__":
    # Anchor 1 (inner corner of L-bracket) is at (-360.158, -234.568)
    anchor_x = -360.158
    anchor_y = -234.568
    anchor_z = -122.20 # bed z in machine position

    generate_thread_milling_gcode(
        filename="case_bottom_thread_milling.gcode",
        center_x_mm=(anchor_x + 12.85),
        center_y_mm=(anchor_y + 33.5),
        center_z_mm=(anchor_z + 19)
    )

    generate_thread_milling_gcode(
        filename="case_top_thread_milling.gcode",
        center_x_mm=(anchor_x + 19.1),
        center_y_mm=(anchor_y + 31),
        center_z_mm=(anchor_z + 94)
    )