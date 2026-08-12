"""
Run as:
python3 pkg/vision/mocap/boards/led/ring_script.py


146.25

160.4
"""

import sys
import os
import math
import pcbnew

BOARD_PATH = 'pkg/vision/mocap/boards/led/latest/board-latest.kicad_pcb'

def place_components_external(start_index, radius_mm, start_orientation, start_angle = 90, angle_sign = 1):
    # --- Configuration ---
    center_x_mm = 124.0
    center_y_mm = 136.0
    count = 12
    ref_prefix = "D"
    
    print(f"Loading...")
    board = pcbnew.LoadBoard(BOARD_PATH)
    assert board

    print(f"Placing {count} components in a ring...")

    for i in range(count):
        ref = f"{ref_prefix}{i + start_index}"
        footprint = board.FindFootprintByReference(ref)
        
        if footprint is None:
            print(f"Warning: {ref} not found.")
            continue

        # --- Calculate Position ---
        # 0 deg = Right, 90 deg = Down (KiCad coordinates)
        angle_step = 360.0 / count
        
        # Current logic: i=0 is at Bottom (90 deg in KiCad)
        # Clockwise means angle INCREASES in KiCad
        pos_angle_deg = start_angle + (angle_sign *  i * angle_step)
        pos_angle_rad = math.radians(pos_angle_deg)

        new_x = center_x_mm + (radius_mm * math.cos(pos_angle_rad))
        new_y = center_y_mm + (radius_mm * math.sin(pos_angle_rad))
        
        new_pos = pcbnew.VECTOR2I(pcbnew.FromMM(new_x), pcbnew.FromMM(new_y))
        footprint.SetPosition(new_pos)

        # --- Calculate Orientation ---
        # Your formula: 90 - i * (360 / 12)
        orient_angle_deg = start_orientation - (angle_sign * i * angle_step)
        orient_angle_deg = orient_angle_deg % 360
        
        footprint.SetOrientation(pcbnew.EDA_ANGLE(orient_angle_deg, pcbnew.DEGREES_T))

        # --- Hide Name (Reference) and Value ---
        # Get the text objects
        ref_text = footprint.Reference()
        val_text = footprint.Value()

        # Set visibility to False
        ref_text.SetVisible(False)
        val_text.SetVisible(False)

        print(f"Updated {ref}")

    # Save the modified board back to disk
    board.Save(BOARD_PATH)
    print(f"Done!")

def replicate_layout():
    # --- Configuration ---
    center_x_mm = 124.0
    center_y_mm = 136.0
    
    # We are replicating D1_OUT -> D2_OUT ... D12_OUT
    source_net_name = "D1_OUT"
    count = 11
    
    # Rotation Step: 360 / 12 = 30 degrees.
    # Positive = Clockwise in KiCad (to match your component placement)
    angle_step = 360.0 / 12 

    # --- Setup ---
    board = pcbnew.LoadBoard(BOARD_PATH)

    # Define the center point object
    center_point = pcbnew.VECTOR2I(pcbnew.FromMM(center_x_mm), pcbnew.FromMM(center_y_mm))

    print(f"Replicating {source_net_name} to {count-1} other nets...")

    # --- Step 1: Identify Source Objects ---
    source_vias = []
    source_zones = []

    # Find Vias
    for track in board.GetTracks():
        # Check if it is a Via and on the source net
        if track.GetClass() == "PCB_VIA" and track.GetNetname() == source_net_name:
            source_vias.append(track)

    # Find Zones (Copper Pours)
    for zone in board.Zones():
        if zone.GetNetname() == source_net_name:
            source_zones.append(zone)

    if not source_vias and not source_zones:
        print(f"No vias or zones found on {source_net_name}!")
        return

    print(f"Found {len(source_vias)} vias and {len(source_zones)} zones on {source_net_name}.")

    # --- Step 2: Loop through D2...D12 ---
    for i in range(1, count):
        # i goes from 1 to 11.
        # Target Net: D2_OUT, D3_OUT, etc.
        target_net_name = f"D{i+1}_OUT"
        
        # Check if net exists
        target_net = board.FindNet(target_net_name)
        if target_net is None:
            print(f"Warning: Net {target_net_name} not found in netlist. Skipping.")
            continue
        
        target_net_code = target_net.GetNetCode()

        # --- Step 3: Clean up existing objects (Re-runnability) ---
        # We must collect items to remove first, then remove them to avoid iterator invalidation
        items_to_remove = []

        for track in board.GetTracks():
            if track.GetClass() == "PCB_VIA" and track.GetNetname() == target_net_name:
                items_to_remove.append(track)
        
        for zone in board.Zones():
            if zone.GetNetname() == target_net_name:
                items_to_remove.append(zone)
        
        for item in items_to_remove:
            board.Remove(item)

        # --- Step 4: Duplicate, Rotate, and Rename ---
        rotation_angle_deg = i * angle_step
        # Create an angle object (KiCad 6+ API)
        rotation_angle = pcbnew.EDA_ANGLE(rotation_angle_deg, pcbnew.DEGREES_T)

        # Process Vias
        for source_via in source_vias:
            new_via = source_via.Duplicate()
            new_via.SetNetCode(target_net_code)
            new_via.Rotate(center_point, rotation_angle)
            board.Add(new_via)

        # Process Zones
        for source_zone in source_zones:
            new_zone = source_zone.Duplicate()
            new_zone.SetNetCode(target_net_code)
            new_zone.Rotate(center_point, rotation_angle)
            
            # Important: You often need to refill zones after moving/rotating
            # But strictly speaking, the geometry is copied. 
            # We set the priority/layer same as source automatically by Duplicate()
            board.Add(new_zone)

        print(f"  Generated {target_net_name} (Rotated {rotation_angle_deg:.1f}°)")

    # --- Finish ---
    # Re-fill all zones to ensure connectivity is calculated correctly
    # filler = pcbnew.ZONE_FILLER(board)
    # filler.Fill(board.Zones()) 
    # (Uncomment the lines above if you want it to auto-refill, 
    #  but usually better to press 'B' in GUI to verify first)

    board.Save(BOARD_PATH)


def remove_silkscreen():
    # --- Configuration ---
    ref_prefix = "D"
    start_index = 1
    end_index = 24  # D1 to D24 inclusive
    
    # --- Setup ---
    board = pcbnew.LoadBoard(BOARD_PATH)

    print(f"Removing silkscreen from {ref_prefix}{start_index} to {ref_prefix}{end_index}...")

    for i in range(start_index, end_index + 1):
        ref = f"{ref_prefix}{i}"
        footprint = board.FindFootprintByReference(ref)
        
        if footprint is None:
            print(f"Warning: {ref} not found.")
            continue
        
        # We must collect items to delete first to avoid modifying the list while iterating
        items_to_remove = []

        # 1. Check Graphical Items (Lines, Arcs, Circles, Polygons)
        for graphic in footprint.GraphicalItems():
            layer = graphic.GetLayer()
            if layer == pcbnew.F_SilkS or layer == pcbnew.B_SilkS:
                items_to_remove.append(graphic)

        # 2. Check Text Items (Reference, Value, and extra text fields)
        # Note: We usually hide Reference/Value rather than delete them to avoid KiCad errors,
        # but if they are on Silk layer, we will set them invisible.
        
        # Check Reference
        if footprint.Reference().GetLayer() in [pcbnew.F_SilkS, pcbnew.B_SilkS]:
            footprint.Reference().SetVisible(False)
            
        # Check Value
        if footprint.Value().GetLayer() in [pcbnew.F_SilkS, pcbnew.B_SilkS]:
            footprint.Value().SetVisible(False)

        # Remove the graphical items found
        for item in items_to_remove:
            footprint.Remove(item)

        print(f"Processed {ref}: Removed {len(items_to_remove)} silk items.")

    # --- Finish ---
    board.Save(BOARD_PATH)

if __name__ == "__main__":
    place_components_external(
        start_index = 1, radius_mm = 10,
        start_orientation = -90 - (360 / 12),
        start_angle = 90 + (360 / 12),
        angle_sign = -1
    )
    # place_components_external(
    #     start_index = 13,
    #     radius_mm = 16,
    #     start_orientation = 360 / 12,
    #     start_angle = 90 - (360 / 12)
    # )

    
    def place_new_led_ring():
        refs = ["D15", "D16", "D17", "D18", "D19", "D20", "D21", "D22", "D23", "D24", "D25", "D27"]
        count = len(refs)
        center_x_mm = 124.0
        center_y_mm = 136.0
        radius_mm = 11
        
        # For the 2nd LED (index 1) to be at bottom (pos_angle = 90) and rotation = 90:
        start_angle = 120
        start_orientation = 60
        angle_sign = -1
        angle_step = 360.0 / count
        
        offset_y_local = -0.405 
        
        print(f"Loading board for new LED ring...")
        board = pcbnew.LoadBoard(BOARD_PATH)
        assert board

        print(f"Placing {count} new LEDs in a ring with radius {radius_mm}mm...")

        for i, ref in enumerate(refs):
            footprint = board.FindFootprintByReference(ref)
            if footprint is None:
                print(f"Warning: {ref} not found.")
                continue

            # Position of the LED center
            pos_angle_deg = start_angle + (angle_sign * i * angle_step)
            pos_angle_rad = math.radians(pos_angle_deg)
            
            led_x = center_x_mm + (radius_mm * math.cos(pos_angle_rad))
            led_y = center_y_mm + (radius_mm * math.sin(pos_angle_rad))
            
            # Orientation of the package
            orient_angle_deg = start_orientation - (angle_sign * i * angle_step)
            orient_angle_deg = orient_angle_deg % 360
            
            # KiCad rotation is clockwise. Mathematical angle is -orient_angle_deg.
            theta_math_rad = math.radians(-orient_angle_deg)
            
            # Rotated offset of the LED center from the package center
            dx = -offset_y_local * math.sin(theta_math_rad)
            dy = offset_y_local * math.cos(theta_math_rad)
            
            # The package is placed such that its offset LED center hits the target
            new_x = led_x - dx
            new_y = led_y - dy
            
            new_pos = pcbnew.VECTOR2I(pcbnew.FromMM(new_x), pcbnew.FromMM(new_y))
            footprint.SetPosition(new_pos)
            
            footprint.SetOrientation(pcbnew.EDA_ANGLE(orient_angle_deg, pcbnew.DEGREES_T))

            ref_text = footprint.Reference()
            val_text = footprint.Value()
            ref_text.SetVisible(False)
            val_text.SetVisible(False)

            print(f"Updated {ref} to x={new_x:.3f}, y={new_y:.3f}, rot={orient_angle_deg}")

        board.Save(BOARD_PATH)
        print(f"Done placing new LED ring!")

    # place_new_led_ring()

    replicate_layout()

    # remove_silkscreen()