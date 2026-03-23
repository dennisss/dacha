import pcbnew
import os
import argparse
import sys

def generate_fpc(out_path):
    # Initialize a new board
    board = pcbnew.BOARD()

    # --- FPC Parameters ---
    NUM_PINS = 22
    PITCH = 0.5
    WIDTH = (NUM_PINS + 1) * PITCH # 11.5 mm
    LENGTH = 38.0 # 40 mm
    
    TRACK_WIDTH = 0.3 # mm
    CONTACT_LEN = 3.0 # mm
    STIFFENER_LEN = 3.5 # mm

    # Helper functions for internal coordinate conversion (nanometers)
    def nm(val): return int(val * 1e6)
    def pt(x, y): return pcbnew.VECTOR2I(nm(x), nm(y))

    # 1. Draw Edge Cuts (Board Outline)
    edge_pts = [(0,0), (WIDTH, 0), (WIDTH, LENGTH), (0, LENGTH)]
    for i in range(4):
        p1, p2 = edge_pts[i], edge_pts[(i+1)%4]
        seg = pcbnew.PCB_SHAPE(board)
        seg.SetShape(pcbnew.SHAPE_T_SEGMENT)
        seg.SetLayer(pcbnew.Edge_Cuts)
        seg.SetStart(pt(p1[0], p1[1]))
        seg.SetEnd(pt(p2[0], p2[1]))
        seg.SetWidth(nm(0.1))
        board.Add(seg)

    # 2. Draw Copper Traces (F.Cu)
    for i in range(NUM_PINS):
        x = PITCH + (i * PITCH)
        track = pcbnew.PCB_TRACK(board)
        track.SetStart(pt(x, 0))
        track.SetEnd(pt(x, LENGTH))
        track.SetWidth(nm(TRACK_WIDTH))
        track.SetLayer(pcbnew.F_Cu)
        board.Add(track)

    # 3. Create Solder Mask Openings (F.Mask)
    def add_mask_opening(y_start, y_end):
        poly = pcbnew.PCB_SHAPE(board)
        poly.SetShape(pcbnew.SHAPE_T_POLY)
        poly.SetLayer(pcbnew.F_Mask)
        
        # Use SHAPE_POLY_SET for modern KiCad compatibility
        poly_set = pcbnew.SHAPE_POLY_SET()
        poly_set.NewOutline()
        poly_set.Append(nm(0), nm(y_start))
        poly_set.Append(nm(WIDTH), nm(y_start))
        poly_set.Append(nm(WIDTH), nm(y_end))
        poly_set.Append(nm(0), nm(y_end))
        
        poly.SetPolyShape(poly_set)
        
        if hasattr(poly, 'SetFilled'):
            poly.SetFilled(True)
        poly.SetWidth(0)
        board.Add(poly)

    add_mask_opening(0, CONTACT_LEN)
    add_mask_opening(LENGTH - CONTACT_LEN, LENGTH)

    # 4. Create JLCPCB Filled Stiffener Regions (Eco1.User)
    def add_stiffener_marker(y_start, y_end):
        poly = pcbnew.PCB_SHAPE(board)
        poly.SetShape(pcbnew.SHAPE_T_POLY)  # Create a polygon
        poly.SetLayer(pcbnew.Eco1_User)
        
        # Define the filled region with SHAPE_POLY_SET
        poly_set = pcbnew.SHAPE_POLY_SET()
        poly_set.NewOutline()
        poly_set.Append(nm(0), nm(y_start))
        poly_set.Append(nm(WIDTH), nm(y_start))
        poly_set.Append(nm(WIDTH), nm(y_end))
        poly_set.Append(nm(0), nm(y_end))
        
        poly.SetPolyShape(poly_set)
        
        # Ensure it is filled and has no stroke
        if hasattr(poly, 'SetFilled'):
            poly.SetFilled(True)
        poly.SetWidth(0) # Zero stroke width
        board.Add(poly)

    # The third argument for text position has been removed
    add_stiffener_marker(0, STIFFENER_LEN)
    add_stiffener_marker(LENGTH - STIFFENER_LEN, LENGTH)

    # 5. Save the output
    try:
        pcbnew.SaveBoard(out_path, board)
        print(f"Success! Custom FPC generated at: {os.path.abspath(out_path)}")
    except Exception as e:
        print(f"Error saving board: {e}")
        sys.exit(1)

if __name__ == "__main__":
    # parser = argparse.ArgumentParser(description="Generate a KiCad FPC cable board file.")
    # parser.add_argument("output_path", help="The full path where the .kicad_pcb file should be saved (e.g., ./my_cable.kicad_pcb)")
    
    # args = parser.parse_args()
    
    # # Ensure the directory exists
    # out_dir = os.path.dirname(os.path.abspath(args.output_path))
    # if out_dir and not os.path.exists(out_dir):
    #     print(f"Error: Directory '{out_dir}' does not exist.")
    #     sys.exit(1)
        
    # generate_fpc(args.output_path)

    generate_fpc("pkg/media/camera/boards/camera_ffc/latest/latest.kicad_pcb")