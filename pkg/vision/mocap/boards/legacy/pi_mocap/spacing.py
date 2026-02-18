import math
import pcbnew

DEG360 = 2 * math.pi

CENTER_X, CENTER_Y = (122, 128)

IR_RADIUS = 10
RGB_RADIUS = 15

board = pcbnew.GetBoard()

def place_led_ring(prefix, radius, num_leds):
    for i in range(num_leds):
        led = board.FindModuleByReference(prefix + str(i + 1))

        angle = DEG360 * (i / num_leds)
        print(angle)

        rel_x = math.cos(angle) * radius
        rel_y = math.sin(angle) * radius

        led_pos = pcbnew.wxPointMM(CENTER_X + rel_x, CENTER_Y + rel_y)
        led.SetPosition(led_pos)
        led.SetOrientationDegrees(-angle * (180 / math.pi) - 90)

def place_caps(radius, num_leds):
    for i in range(num_leds):
        cap = board.FindModuleByReference('C' + str(i + 1))

        angle = DEG360 * (i / num_leds) + (15/360) * DEG360
        print(angle)

        rel_x = math.cos(angle) * radius
        rel_y = math.sin(angle) * radius

        cap_pos = pcbnew.wxPointMM(CENTER_X + rel_x, CENTER_Y + rel_y)
        cap.SetPosition(cap_pos)
        cap.SetOrientationDegrees(-angle * (180 / math.pi))


# place_led_ring('IR', IR_RADIUS, 12)

# place_led_ring('RGB', RGB_RADIUS, 12)

place_caps(RGB_RADIUS, 12)

pcbnew.Refresh()