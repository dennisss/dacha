import pcbnew
import json

KEYBOARD_CASE_WIDTH = 365
KEYBOARD_CASE_HEIGHT = 130

# TODO: Derive from the layout file.
KEYBOARD_WIDTH_UNITS = 18.25
KEYBOARD_HEIGHT_UNITS = 6.5

CHANGE = False

START_X_MM = 100
START_Y_MM = 100

DIODE_RELATIVE_X_MM = -7.085
DIODE_RELATIVE_Y_MM = 3.75

GRID_WIDTH = 16
GRID_HEIGHT = 6

UNIT_TO_MM = 19.05

with open('/home/dennis/workspace/dacha/doc/keyboard/keyboard-layout.json') as f:
    data = json.load(f)

board = pcbnew.GetBoard()


SIDE_LED_HEIGHT = 70
SIDE_LED_START = 88
SIDE_LED_CAP_X = 1

def place_side_leds():
    for i in range(8, 16):
        led_index = SIDE_LED_START + i

        flipped = False
        relative_i = i
        if i >= 8:
            relative_i -= 8
            flipped = True

        if flipped:
            led_x = START_X_MM + KEYBOARD_CASE_WIDTH - 6
            # led_y = START_Y_MM + (KEYBOARD_CASE_HEIGHT / 2) - (SIDE_LED_HEIGHT / 2) + (relative_i * (SIDE_LED_HEIGHT / 7))
        else:
            led_x = START_X_MM + 6
        led_y = START_Y_MM + (KEYBOARD_CASE_HEIGHT / 2) + (SIDE_LED_HEIGHT / 2) - (relative_i * (SIDE_LED_HEIGHT / 7))

        led = board.FindModuleByReference('E' + str(led_index))
        led.SetPosition(pcbnew.wxPointMM(led_x, led_y))

        led_cap = board.FindModuleByReference('CS' + str(led_index))

        if flipped:
            led.SetOrientation(2700)    
            led_cap.SetOrientation(900)
            led_cap.SetPosition(pcbnew.wxPointMM(led_x - SIDE_LED_CAP_X, led_y))
            link_via_to_pad(led_cap, '1', 0, 2)
            link_via_to_pad(led_cap, '2', 0, -2)
        else:
            led.SetOrientation(900)
            led_cap.SetOrientation(2700)
            led_cap.SetPosition(pcbnew.wxPointMM(led_x + SIDE_LED_CAP_X, led_y))
            link_via_to_pad(led_cap, '1', 0, -2)
            link_via_to_pad(led_cap, '2', 0, 2)

def get_key_index(row_i, col_i):
    # Overflow from second row
    if row_i == 1 and col_i == 16:
        row_i = 3
        col_i = 15
    # Overflow from third row
    elif row_i == 2 and col_i == 16:
        row_i = 4
        col_i = 15
    # Space Bar
    elif row_i == 5 and col_i == 3:
        col_i = 5
    # Shift right buttons after space.
    elif row_i == 5 and col_i > 3:
        col_i += 5
    # Up Arrow
    elif row_i == 4 and col_i == 12:
        col_i = 14
    # elif row_i == 3 and col_i == 13:
    #     col_i = 14

    return (row_i * GRID_WIDTH + col_i) + 1

def connect_lower(switch, diode):
    switch_pad = None
    for p in switch.Pads():
        if p.GetName() == '1':
            switch_pad = p
            break

    diode_pad = None
    for p in diode.Pads():
        if p.GetName() == '2':
            diode_pad = p
            break
    assert(diode_pad.GetNetCode() == switch_pad.GetNetCode())

    switch_pt = switch_pad.GetBoundingBox().Centre()
    diode_pt = diode_pad.GetBoundingBox().Centre()

    track = pcbnew.TRACK(board)
    track.SetNet(diode_pad.GetNet())
    track.SetLayer(31)
    track.SetStart(switch_pt)
    track.SetEnd(diode_pt)
    track.SetWidth(pcbnew.FromMM(0.4))
    board.Add(track)


def link_via_to_pad(footprint, pad_name, rel_x_mm, rel_y_mm):
    pad = None
    for p in footprint.Pads():
        if p.GetName() == pad_name:
            pad = p
            break

    footprint_pt = pad.GetBoundingBox().Centre()

    via_pt = pcbnew.wxPointMM(pcbnew.ToMM(footprint_pt.x) + rel_x_mm, pcbnew.ToMM(footprint_pt.y) + rel_y_mm)

    via = pcbnew.VIA(board)
    via.SetLayerPair(0, 31)
    via.SetPosition(via_pt)
    via.SetDrill(pcbnew.FromMM(0.5))
    via.SetWidth(pcbnew.FromMM(1))
    via.SetNet(pad.GetNet())
    via.SetViaType(pcbnew.VIA_THROUGH)
    board.Add(via)

    track = pcbnew.TRACK(board)
    track.SetNet(pad.GetNet())
    track.SetLayer(31)
    track.SetStart(footprint_pt)
    track.SetEnd(via_pt)
    track.SetWidth(pcbnew.FromMM(0.4))
    board.Add(track)


def connect_upper(switch):
    link_via_to_pad(switch, '2', 2.5, 0)

def place_main_keys():
    current_y_units = 0
    used_key_indices = set()

    key_start_x = START_X_MM + (KEYBOARD_CASE_WIDTH - (KEYBOARD_WIDTH_UNITS * UNIT_TO_MM)) / 2
    key_start_y = START_Y_MM + (KEYBOARD_CASE_HEIGHT - (KEYBOARD_HEIGHT_UNITS * UNIT_TO_MM)) / 2

    all_key_positions = []
    flat_key_positions = []

    for row_i in range(len(data)):
        row = data[row_i]

        current_x_units = 0
        current_key_width = 1

        row_key_positions = []

        col_i = 0
        for i in range(len(row)):
            val = row[i]
            if isinstance(val, str):
                # print((current_y_units, current_x_units))

                key_index = get_key_index(row_i, col_i)
                assert(key_index not in used_key_indices)
                used_key_indices.add(key_index)

                center_x_mm = (current_x_units + (current_key_width / 2)) * UNIT_TO_MM + key_start_x
                center_y_mm = (current_y_units + (1 / 2)) * UNIT_TO_MM + key_start_y

                # The wireless toggle switch.
                if row_i == 3 and col_i == 14:
                    center_x_mm = 461.75

                if row_i != 3 or col_i < 13:
                    row_key_positions.append((center_x_mm, center_y_mm))
                    flat_key_positions.append([round(center_x_mm, 4), round(center_y_mm, 4), current_key_width])

                # Print out index to name mapping for using in code.
                print(str(key_index) + ' = ' + val)

                # print('Place ' + str(key_index))
                if board is not None and CHANGE:
                    print('@ ' + str(center_x_mm) + ', ' + str(center_y_mm))
                    switch = board.FindModuleByReference('SW' + str(key_index))
                    switch.SetPosition(pcbnew.wxPointMM(center_x_mm, center_y_mm))

                    diode = board.FindModuleByReference('D' + str(key_index))
                    diode_pos = pcbnew.wxPointMM(center_x_mm + DIODE_RELATIVE_X_MM, center_y_mm + DIODE_RELATIVE_Y_MM)
                    diode.SetPosition(diode_pos)
                    if not diode.IsFlipped():
                        diode.Flip(diode_pos)
                    diode.SetOrientation(900)

                    connect_lower(switch, diode)
                    connect_upper(switch)

                    link_via_to_pad(diode, '1', 0, 2)

                current_x_units += current_key_width

                # Reset variables that only apply for a single key.
                current_key_width = 1


                col_i += 1
            else:
                found_something = False
                if 'y' in val:
                    current_y_units += val['y']
                    found_something = True
                if 'w' in val:
                    current_key_width = val['w']
                    found_something = True
                if 'x' in val:
                    current_x_units += val['x']
                    found_something = True

                assert(found_something)

        all_key_positions.append(row_key_positions)

        current_y_units += 1

    print(flat_key_positions)

    # Do all LEDs
    current_led_index = 1
    flipped = False
    for row_key_positions in all_key_positions:
        scale = 1
        if flipped:
            scale = -1
            row_key_positions.reverse()

        for (center_x_mm, center_y_mm) in row_key_positions:
            if CHANGE:
                led = board.FindModuleByReference('E' + str(current_led_index))
                led_pos = pcbnew.wxPointMM(center_x_mm + 0, center_y_mm + 5.08)
                led.SetPosition(led_pos)

                if flipped:
                    led.SetOrientation(0)
                else:
                    led.SetOrientation(1800)
                link_via_to_pad(led, '1', scale * 2, scale * 0)
                link_via_to_pad(led, '3', scale * -2, scale * 0)

            led_cap = board.FindModuleByReference('CS' + str(current_led_index))
            if flipped:
                link_via_to_pad(led_cap, '2', -1.6, 0)
                led_cap.SetOrientation(900)            
            else:
                link_via_to_pad(led_cap, '1', -1.6, 0)
                led_cap.SetOrientation(2700)
            led_cap.SetPosition(pcbnew.wxPointMM(center_x_mm + 6.106, center_y_mm + 4.34))

            current_led_index += 1


        flipped = not flipped


    # Move all unused switches and diodes to 0,0
    for key_index in range(1, (GRID_WIDTH * GRID_HEIGHT) + 1):
        if key_index not in used_key_indices:
            if CHANGE:
                switch = board.FindModuleByReference('SW' + str(key_index))
                switch.SetPosition(pcbnew.wxPointMM(0, 0))
                diode = board.FindModuleByReference('D' + str(key_index))
                diode.SetPosition(pcbnew.wxPointMM(0, 0))

place_main_keys()
place_side_leds()

pcbnew.Refresh()

print('Done!')
