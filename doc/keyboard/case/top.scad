// Notes:
// - z=0 is on the top of the PCB.


start_x = 100;
start_y = -100;

outer_width = 365;
outer_height = 130;
outer_extra_pad = 0.4;

unit_dim = 19.05;
key_hole_dim = 14;
key_hole_pad = 0.1;

// 
plate_depth = 1.6;

// Distance from the top of the plate to the pcb (which is below the plate)
// According to Cherry MX the distance is 5mm but we add some space for foam pads (which are usually 0.5mm)
plate_to_pcb_depth = 5.4;

wall_width = 3*0.45;
outer_wall_width = 0.9;

pcb_depth = 1.6;

// Width of the 
stabilizer_wire_width = 1.6;

oled_header_x = 400.975;
oled_header_y = 179.2875;
oled_outer_width = 38.2;

oled_display_width = oled_outer_width - 2*5;
oled_padding = 0.2;

hex_nut_radius = 4.2 / 2;
hex_nut_height = 2.2;

$fn = 40;

module Hexagon(r) {
  function angle(i) = (i * 360 / 6);
  
  polygon([
    for (i = [0:6]) [r*cos(angle(i)), r*sin(angle(i))]
  ]);
}

module HexagonProtrusion(r, p) {
  function angle(i) = (i * 360 / 6);
  function point(i) = [r*cos(angle(i)), r*sin(angle(i))];
  
  polygon([
    point(-1),
    [point(-1)[0] + p, point(-1)[1]],
    [point(1)[0] + p, point(1)[1]],
    point(1)
  ]);
}

module OLEDCutout() {
  union() {
    translate([oled_header_x - 1.5 + (oled_outer_width / 2), -oled_header_y, 0])
    cube([oled_display_width + oled_padding, 12.2 + oled_padding, 100], center=true);
    
    translate([oled_header_x - 1.5 + (oled_outer_width / 2), -oled_header_y, plate_to_pcb_depth - 1 - 0.4])
    cube([oled_outer_width + 2*oled_padding, 12.2 + oled_padding, 2], center=true);
  }
}


// A whole for inserting a keyswitch centered at (0, 0)
// This is meant to be extruded out of the main plate.
module KeyHole() {
  translate([
    0,
    0,
    (plate_to_pcb_depth - (plate_depth / 2))
  ])
  cube([key_hole_dim+key_hole_pad, key_hole_dim+key_hole_pad,
        100 // Remove through all
  ], center=true);
}

// Wall around each keyswitch which makes contact with the upper plate and the PCB.
// NOTE: The height is the wall is (plate_to_pcb_depth - plate_depth) but we
// define it here as plate_to_pcb_depth to ensure that it overlaps with the plate
module KeyWall(key_unit_width) {
  translate([
    0,
    0,
    plate_to_pcb_depth / 2
  ]) {
    difference() {
      cube([
        key_unit_width*unit_dim + wall_width, unit_dim + wall_width,
        plate_to_pcb_depth], center=true);
      cube([
        key_unit_width*unit_dim - wall_width, unit_dim - wall_width,
        // +0.1 to make it slightly larger than the first shape
        plate_to_pcb_depth + 0.1], center=true);  
    }  
  }
}

module Plate() {
  translate([start_x, start_y, plate_to_pcb_depth])
  translate([0, -outer_height, -plate_depth])
  cube([outer_width, outer_height, plate_depth], center = false);
}

module PlateWall(bottom_z, wall_height) {
  // Extra padding to deal with small inaccuracies
  wall_outer_width = outer_width + outer_extra_pad;
  wall_outer_height = outer_height + outer_extra_pad;
  
  translate([start_x - outer_extra_pad/2, start_y + outer_extra_pad/2, bottom_z])
  translate([wall_outer_width / 2, -wall_outer_height / 2, wall_height / 2])
  difference() {
    cube([wall_outer_width, wall_outer_height, wall_height], center=true);
    cube([wall_outer_width - 2*outer_wall_width, wall_outer_height - 2*outer_wall_width, 100], center=true);
  }
}

key_centers = [[118.1937, 112.6125, 1], [156.2937, 112.6125, 1], [175.3438, 112.6125, 1], [194.3938, 112.6125, 1], [213.4437, 112.6125, 1], [242.0187, 112.6125, 1], [261.0688, 112.6125, 1], [280.1187, 112.6125, 1], [299.1687, 112.6125, 1], [327.7437, 112.6125, 1], [346.7937, 112.6125, 1], [365.8438, 112.6125, 1], [384.8938, 112.6125, 1], [408.7063, 112.6125, 1], [427.7563, 112.6125, 1], [446.8062, 112.6125, 1], [118.1937, 141.1875, 1], [137.2437, 141.1875, 1], [156.2937, 141.1875, 1], [175.3438, 141.1875, 1], [194.3938, 141.1875, 1], [213.4437, 141.1875, 1], [232.4937, 141.1875, 1], [251.5437, 141.1875, 1], [270.5938, 141.1875, 1], [289.6437, 141.1875, 1], [308.6938, 141.1875, 1], [327.7437, 141.1875, 1], [346.7937, 141.1875, 1], [375.3687, 141.1875, 2], [408.7063, 141.1875, 1], [427.7563, 141.1875, 1], [446.8062, 141.1875, 1], [122.9562, 160.2375, 1.5], [146.7687, 160.2375, 1], [165.8187, 160.2375, 1], [184.8687, 160.2375, 1], [203.9187, 160.2375, 1], [222.9688, 160.2375, 1], [242.0187, 160.2375, 1], [261.0688, 160.2375, 1], [280.1187, 160.2375, 1], [299.1687, 160.2375, 1], [318.2188, 160.2375, 1], [337.2688, 160.2375, 1], [356.3188, 160.2375, 1], [380.1313, 160.2375, 1.5], [408.7063, 160.2375, 1], [427.7563, 160.2375, 1], [446.8062, 160.2375, 1], [125.3375, 179.2875, 1.75], [151.5312, 179.2875, 1], [170.5812, 179.2875, 1], [189.6312, 179.2875, 1], [208.6812, 179.2875, 1], [227.7312, 179.2875, 1], [246.7812, 179.2875, 1], [265.8312, 179.2875, 1], [284.8813, 179.2875, 1], [303.9312, 179.2875, 1], [322.9812, 179.2875, 1], [342.0312, 179.2875, 1], [372.9875, 179.2875, 2.25], [130.1, 198.3375, 2.25], [161.0562, 198.3375, 1], [180.1062, 198.3375, 1], [199.1562, 198.3375, 1], [218.2063, 198.3375, 1], [237.2562, 198.3375, 1], [256.3062, 198.3375, 1], [275.3562, 198.3375, 1], [294.4062, 198.3375, 1], [313.4562, 198.3375, 1], [332.5063, 198.3375, 1], [368.225, 198.3375, 2.75], [427.7563, 198.3375, 1], [120.575, 217.3875, 1.25], [144.3875, 217.3875, 1.25], [168.2, 217.3875, 1.25], [239.6375, 217.3875, 6.25], [311.075, 217.3875, 1.25], [334.8875, 217.3875, 1.25], [358.7, 217.3875, 1.25], [382.5125, 217.3875, 1.25], [408.7063, 217.3875, 1], [427.7563, 217.3875, 1], [446.8062, 217.3875, 1]]
;

screw_holes = [

  // Center
  [213.5, 151],
  [208.7, 189.6],
  // Center-right
  //[282.5, 127],
  [346.8, 151],
  [346.8, 198.5],
  [406.5, 198.5],
  [446.8, 127],

];

center_screw_holes = [
  [282.5, 127],
  [282.5, 216.9]
];

screw_holes_with_nuts = [
  // Left
  [104.5, 112],
  [104.5, 165],
  [104.9, 218],
  
    [460.5, 112],
  [460.5, 165],
  [460.5, 218],
];

// NOTE: Space bar will have the wire on top while others have it on the bottom side 
stabilizer_holes = [
  // Left Shift
  [118.162, 191.3525],
  [118.162, 206.5925],
  [142.038, 191.3525],
  [142.038, 206.5925],
  // Space Bar
  [189.6376, 209.1325],
  [189.6376, 224.3725],
  [289.6374, 209.1325],
  [289.6374, 224.3725],
  // Right Shift
  [356.287, 191.3525],
  [356.287, 206.5925],
  [380.163, 191.3525],
  [380.163, 206.5925],
  // Enter
  [361.0495, 172.3025],
  [361.0495, 187.5425],
  [384.9255, 172.3025],
  [384.9255, 187.5425],
  // Backspace
  [363.43075, 134.2025],
  [363.43075, 149.4425],
  [387.30675, 134.2025],
  [387.30675, 149.4425],
];


// x1, x2, y
stabilizer_wires = [
  [118.162, 142.038, 206.5925],
  [189.6376, 289.6374, 209.1325],
  [356.287, 380.163, 206.5925],
  [361.0495, 384.9255, 187.5425],
  [363.43075, 387.30675, 149.4425]
];

function stab_center(i) = [
  stabilizer_holes[i][0],
  -(stabilizer_holes[i][1] + stabilizer_holes[i+1][1]) / 2,
  0
];

module ScrewHole(cut = 0) {
  translate([0, 0, plate_to_pcb_depth / 2])
  // 4.7 is 3 0.45mm walls
  difference() {
    cylinder(h=plate_to_pcb_depth - cut, d=4.7, center=true);
    cylinder(h=100, d=2, center=true);
  }
}



module ScrewHoleWithNut() {
  difference() {
    translate([0, 0, plate_to_pcb_depth / 2])
      cube([8, 8, plate_to_pcb_depth], center = true);
    cylinder(h=100, d=2.4, center=true);
    
    translate([0, 0, (-hex_nut_height / 2) + (plate_to_pcb_depth - plate_depth)])
    cube([hex_nut_radius*2, 50, hex_nut_height], center=true);
    
    /*
    rotate(-90)
    translate([0, 0, (-hex_nut_height / 2) + (plate_to_pcb_depth - plate_depth)])
    linear_extrude(height = hex_nut_height, center = true)
    union() {
      Hexagon(hex_nut_radius);
      HexagonProtrusion(hex_nut_radius, 10);
    } 
    */   
  }
}

// The base of one stabilizer is 20mm high (the amount of space touching the PCB)
// But, further up, the part that will actually need to protrude out of the plate is only 11.2mm high.
// NOTE: This is meant to be drawn at the center point between the stabilizer wholes.
// Normally we assume that the smaler brass screw hole is on top (unless inverted)
module StabHole(inverted) {
  // The center stem is 1mm closer to the center of the small screw hole than the other one. 
  offset = inverted? -1 : 1;
  
  translate([0, offset, 0])
  cube([7.2, 11.4, 100], center=true);
}
module StabClearance() {
  cube([7.2, 22, 100], center=true);
}

module StabWireClearance(data) {
  translate([data[0], -data[2], 0])
  cube([data[1] - data[0], 3*stabilizer_wire_width, 100], center=false);
}

module TopPlateOnePiece() {
  union() {
    
    difference() {
      union() {
        for (key_i = [0:len(key_centers)-1]) {
          translate([key_centers[key_i][0], -key_centers[key_i][1], 0])
          KeyWall(key_centers[key_i][2]);
        }      
      }
      
      // Remove screw holes from walls
      union() {
        for (hole_i = [0:len(screw_holes)-1]) {
          translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], 0])
          cylinder(h=100, d=2, center=true);
        }
      }
      
      union() {
        for (stab_i = [0:2:len(stabilizer_holes)-1]) {
          translate(stab_center(stab_i)) StabClearance();
        }
      }
      
      // Stab wire clearance
      union() {
        for (stab_i = [0:len(stabilizer_wires)-1]) {
          StabWireClearance(stabilizer_wires[stab_i]);
        }
      }
    }

    difference() {
      Plate();
      for (key_i = [0:len(key_centers)-1]) {
        translate([key_centers[key_i][0], -key_centers[key_i][1], 0])
        KeyHole();
      }
      for (stab_i = [0:2:len(stabilizer_holes)-1]) {
        translate(stab_center(stab_i)) StabHole();
      }
      
      // Cut out for the single tactile button
      translate([446.75625, -179.3375, 0])
      cylinder(d=5, h=100, center=true);
      
      OLEDCutout();
    }
    
    for (hole_i = [0:len(screw_holes)-1]) {
      translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], 0])
      ScrewHole();
    }
    
    for (hole_i = [0:len(screw_holes_with_nuts)-1]) {
      translate([screw_holes_with_nuts[hole_i][0],-screw_holes_with_nuts[hole_i][1], 0])
      ScrewHoleWithNut();
    }
    
    PlateWall(
      wall_height = plate_to_pcb_depth + pcb_depth,
      bottom_z = -pcb_depth
    );
  }
}

keyboard_center_x = start_x + (outer_width / 2);
keyboard_center_y = start_y - (outer_height / 2);

// 0.4 is the thickness of the plate around the OLED area
oled_support_height = plate_to_pcb_depth - 0.4 - 2.8;
oled_height = 12.4;
// by 4mm
module OLEDHolder() {
  difference() {
    cube([4, oled_height + 2*0.9, oled_support_height + pcb_depth], center = false);
    
    translate([0, 0.9, oled_support_height])
    cube([100, oled_height, 100], center = false);
  }
}

module Slicer() {
  translate([keyboard_center_x, keyboard_center_y, 0])
  rotate([0, -45, 0])
  cube([0.05, 200, 200], center = true);
}

module TopPlate() {
  union() {
    difference() {
      TopPlateOnePiece();

      // Slice it in half to make it printable in two pieces.
      // Slicing it diagonally to make it easier to fuse and make it less
      // noticeable that there is a seam.
      Slicer();
    }
  
    // Screw holes in the very horizontal middle need to only be added to one side of the cut plate.
    union() {
      for (hole_i = [0:len(center_screw_holes)-1]) {
        translate([
          center_screw_holes[hole_i][0], -center_screw_holes[hole_i][1], 0
        ])
        // These don't quite touch the pcb so that they compress the two sides
          // together a bit.
        // NOTE: The cut is centered
        ScrewHole(cut = 0.8);
      }
    }
  }
}

// Positions of each side illuminating LED on the left side of the PCB.
led_positions = [
  [106, 130],
  [106, 140],
  [106, 150],
  [106, 160],
  [106, 170],
  [106, 180],
  [106, 190],
  [106, 200],
];

diffuser_depth = 4;
diffuser_height = outer_height- 40;
diffuser_outer_height = 115;

module SideDiffuserCutout() {
  pad = 0.1;
  
  translate([start_x, start_y - (outer_height / 2), -pcb_depth - diffuser_depth - 0.2])
  translate([0, 0, 100 / 2])
  cube([10, diffuser_height + pad, 100], center=true);
}

module SideDiffuser() {
  pad = outer_extra_pad / 2;
  
  total_height = diffuser_outer_height;
  total_width = 8;

  union() {
    difference() {  
      translate([start_x - pad, keyboard_center_y])
      translate([0, -total_height / 2, -diffuser_depth - pcb_depth])
      cube([total_width  + pad, total_height, diffuser_depth]);
    
      // Remove screw holes
      for (hole_i = [0:len(screw_holes_with_nuts)-1]) {
        translate([screw_holes_with_nuts[hole_i][0],-screw_holes_with_nuts[hole_i][1], 0])
        cylinder(d=2.6, h=100, center=true);
      }
      
      led_pad = 1;
      
      // Remove minimum amount of space to position the leds
      for (i = [0:len(led_positions)-1]) {
        translate([led_positions[i][0], -led_positions[i][1], 0])
        translate([-(1.5 + led_pad) / 2, -(4 + led_pad) / 2, -50])
        cube([1.5 + led_pad, 4 + led_pad, 100], center=false); // NOTE: The leds are actually 2mm tall 
      }
      
      for (i = [0:len(led_positions)-1]) {
        translate([led_positions[i][0], -led_positions[i][1], -pcb_depth])
        scale([0.8, 1.2, 1])
        sphere(r=5, $fn=50);
      }
      
      translate([start_x + total_width, keyboard_center_y, 0])
      cube([6, 90, 100], center=true);
      
      
      translate([start_x, start_y, -50])
      cube([3, 40, 100], center = true);
      // Mirror of above
      translate([start_x, start_y - outer_height, -50])
      cube([3, 40, 100], center = true);
    }
  
    difference() {
      translate([104.5, -165, -diffuser_depth - pcb_depth])
      cylinder(d=4.4, h=diffuser_depth, center=false);

      translate([104.5, -165, -diffuser_depth - pcb_depth])
      cylinder(d=2.6, h=100, center=false);  
    }
  }
}

// Amount of clearance we must reserve under the PCB for surface mounted components and the screws attaching the pcb to the top
pcb_component_pad = 2;

bottom_plate_depth = 0.8;

battery_height = 7.4;

lowest_z = -(pcb_depth + pcb_component_pad + battery_height + bottom_plate_depth);
echo("Total Height: ", lowest_z + plate_to_pcb_depth);

module BottomPlateMain() {
  translate([
    start_x,
    start_y,
    lowest_z
  ])
  // Align top-left corner at (0,0)
  translate([
    -(outer_extra_pad / 2),
    -(outer_height + outer_extra_pad - (outer_extra_pad / 2)),
    0
  ])
  cube([ outer_width + outer_extra_pad, outer_height + outer_extra_pad, bottom_plate_depth ], center=false);  
}

power_switch_width = 12.8;
power_switch_height = 3; // 6.5; (only about half of the switch is supported.
power_switch_depth = 7;
power_switch_x = (299.16875 + 327.74375) / 2;
power_switch_toggle_depth = 4.6;
power_switch_toggle_width = 7;

power_switch_lift = -(lowest_z + pcb_depth) - power_switch_depth;

module PowerSwitchSupport() {
  wall_width = 0.45 * 3;
  wall_height = power_switch_height - 0.4;

  difference() {
    translate([
      power_switch_x,
      start_y + (outer_extra_pad / 2) - outer_wall_width,
      lowest_z,
    ])
    // Center along x, along y with top side
    translate([
      -((power_switch_width + 2*wall_width)/ 2), -power_switch_height, 0
    ])
    cube([
      power_switch_width + 2*wall_width,
      power_switch_height,
      power_switch_lift + wall_height
    ], center = false);


    translate([
      power_switch_x,
      start_y + (outer_extra_pad / 2) - outer_wall_width,
      power_switch_lift,
    ])
    translate([0, 0, lowest_z])
    // Center along x, along y with top side
    translate([
      -(power_switch_width/ 2), -power_switch_height, 0
    ])
    cube([
      power_switch_width,
      power_switch_height,
      100
    ], center = false);  
  }
}

module PowerSwitchToggleHole() {
  translate([
    power_switch_x, start_y, lowest_z + power_switch_lift + (power_switch_depth / 2)
  ])
  cube([ power_switch_toggle_width, 50, power_switch_toggle_depth ], center=true);
}

module PowerCableCutout() {
  center_x = 227.75;
  width = 21;
  depth = 4.6;
  
  translate([
    center_x, start_y, -depth/2 - pcb_depth
  ])
  cube([
    width, 50, depth
  ], center=true);
  
}


center_x = start_x + (outer_width / 2);
center_y = start_y - (outer_height / 2);

module BottomCornerSupport() {
  pad = 0.1;
  y = ((outer_height - diffuser_outer_height) / 2) - pad;
  
  union() {
    translate([start_x, start_y - y, lowest_z])
    cube([8.1, y, (-lowest_z - pcb_depth - 0.2)]);

    translate([start_x, start_y - (y + 12), lowest_z])
    cube([8.1, (y + 12), (-lowest_z - pcb_depth - diffuser_depth - 0.2)]);
  }
}

module BottomSideMiddleSupport() {
  height = 6;
  translate([start_x, start_y - (outer_height / 2) - (height / 2), lowest_z])
  cube([8.1, height, (-lowest_z - pcb_depth - diffuser_depth - 0.2)]);
}


rib_width = 3*0.45;
module BottomRib(x) {
  
  translate([start_x + x, start_y - (outer_height / 2), lowest_z])
  translate([0, 0, (-lowest_z - pcb_depth - 0.2) / 2])
  cube([rib_width, outer_height, (-lowest_z - pcb_depth - 0.2)], center=true);
}

module SmallBottomRib(x) {
  translate([start_x + x, start_y - (outer_height / 2), lowest_z])
  translate([0, 0, (-lowest_z - pcb_depth - pcb_component_pad) / 2])
  cube([rib_width, outer_height, (-lowest_z - pcb_depth - pcb_component_pad)], center=true);
}

module HorizontalBottomRib(y) {
  depth = (-lowest_z - pcb_depth - pcb_component_pad); 
  translate([
    center_x, start_y - y, lowest_z + (depth / 2)
  ])
  cube([outer_width - 2*8.1, 3*0.45,
    depth
  ], center=true);
}


module BatteryCagePart() {
  height = 61;
  width = 37;
  wall_width = 0.45*3;
  
  difference() {
    union() {
    
      translate([ -(width / 2) - wall_width, -(height/2) - wall_width, 0])
      translate([0, 0, lowest_z])
      cube([8, 8, 6], center=false);
      
    }
    
    cube([width, height, 100], center=true);
  }
}

module BatteryCage() {
  union() {
    BatteryCagePart();
    mirror([1, 0, 0]) BatteryCagePart();
    mirror([0, 1, 0]) BatteryCagePart();
    mirror([0, 1, 0]) mirror([1, 0, 0]) BatteryCagePart();
  }
}

rubber_pad_width = 40;
rubber_pad_height = 10;
// rubber_pad_depth = 1;

module RubberPadRecess() {
  wall_width = 6*0.45;
  
  translate([start_x + 35,  start_y - 15])
  translate([0, 0, 2.2 / 2 + lowest_z])
  cube([rubber_pad_width + 2*wall_width , rubber_pad_height + 2*wall_width , 2.2], center = true);
}

module RubberPadHole() {
  translate([start_x + 35,  start_y - 15])
  translate([0, 0, 1.2 / 2 + lowest_z])
  cube([rubber_pad_width, rubber_pad_height, 1.2], center=true);
}

module MirrorAroundCenter(m1, m2) {
  translate([center_x, center_y, 0]) mirror(m1) mirror(m2) translate([-center_x, -center_y, 0]) children(0);
}

module AllCorners() {
  union() {
    children(0);
    MirrorAroundCenter([1, 0, 0], [0, 0, 0]) children(0);
    MirrorAroundCenter([0, 1, 0], [0, 0, 0]) children(0);
    MirrorAroundCenter([0, 1, 0], [1, 0, 0]) children(0);
  }
}


/*
NOTE: Everything in the bottom plate is calculated to leave 0.2mm below the bottom of the PCB empty (1 3d-printed layer) to allow for some room for compression.
*/
module BottomPlateOnePiece() {
  union() {
  difference() {
    union() {
      difference() {
        union() {
          BottomPlateMain();
          PlateWall(bottom_z=lowest_z, wall_height=(-lowest_z - pcb_depth - 0.2));
          PowerSwitchSupport();
          
          AllCorners() BottomCornerSupport();

          BottomSideMiddleSupport();
          
          translate([center_x, center_y, 0]) mirror([1, 0, 0]) translate([-center_x, -center_y, 0]) BottomSideMiddleSupport();
          
          BottomRib(8.1 + (rib_width / 2));
          BottomRib(outer_width - (8.1 + (rib_width / 2)));
          BottomRib(296);
          SmallBottomRib(240);
          
          SmallBottomRib(outer_width / 6);
          SmallBottomRib(2.3*outer_width / 6);
          
          HorizontalBottomRib(94);
          HorizontalBottomRib(31.6);
          
          translate([(406.5 + 446.8) / 2, -(198.5 + 127) / 2, 0])
          BatteryCage();
          
          AllCorners() RubberPadRecess();
        }
        
        // Cutout for wires coming out of the top of the battery.
        translate([(406.5 + 446.8) / 2, -(198.5 + 127) / 2, -pcb_depth - pcb_component_pad])
        rotate([-90, 0, 0])
        cylinder(d=6, h=(40));
        
        // Cutout for 2-pin JST connector connecting the rocker to the battery.
        translate([335, -130.5, -pcb_depth - 9.4])
        cube([10, 10, 8]);
        
        // Cutout for allowing the battery connector to be plugged into the PCB.
        translate([234, -126.9, -pcb_depth - 9.4 + 4])
        cube([20, 8.4, 8], center=true);
        
        AllCorners() RubberPadHole();
        
        PowerSwitchToggleHole();
        Slicer();
        PowerCableCutout();
        SideDiffuserCutout();

        translate([center_x, center_y, 0]) mirror([1, 0, 0]) translate([-center_x, -center_y, 0]) SideDiffuserCutout();

        // Remove screw holes
        for (hole_i = [0:len(screw_holes_with_nuts)-1]) {
          translate([screw_holes_with_nuts[hole_i][0],-screw_holes_with_nuts[hole_i][1], 0])
          cylinder(d=2.6, h=100, center=true);
        }
      }
   
      for (hole_i = [0:len(screw_holes)-1]) {
        translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], lowest_z])
        cylinder(d=6, h=(-lowest_z - pcb_depth - 0.2));
      } 
      for (hole_i = [0:len(center_screw_holes)-1]) {
        translate([center_screw_holes[hole_i][0],-center_screw_holes[hole_i][1], lowest_z])
        cylinder(d=6, h=(-lowest_z - pcb_depth - 0.2));
      }
      
      for (hole_i = [0:len(screw_holes)-1]) {
        translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], lowest_z])
        cylinder(d=8.5, h=(bottom_plate_depth + 4));
      } 
      for (hole_i = [0:len(center_screw_holes)-1]) {
        translate([center_screw_holes[hole_i][0],-center_screw_holes[hole_i][1], lowest_z])
        cylinder(d=8.5, h=(bottom_plate_depth + 4));
      }
      
     
    }
    
    // Remove holes from screws
    for (hole_i = [0:len(screw_holes)-1]) {
      translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], lowest_z])
      cylinder(d=2.4, h=100, center=true);
    } 
    for (hole_i = [0:len(center_screw_holes)-1]) {
      translate([center_screw_holes[hole_i][0],-center_screw_holes[hole_i][1], lowest_z])
      cylinder(d=2.4, h=100, center=true);
    }
    
    // Remove recess for screw head
    for (hole_i = [0:len(screw_holes)-1]) {
      translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], lowest_z])
      cylinder(d=4, h=2.6, center=false);
    } 
    for (hole_i = [0:len(center_screw_holes)-1]) {
      translate([center_screw_holes[hole_i][0],-center_screw_holes[hole_i][1], lowest_z])
      cylinder(d=4, h=2.6, center=false);
    }
    
    for (hole_i = [0:len(screw_holes_with_nuts)-1]) {
      translate([screw_holes_with_nuts[hole_i][0],-screw_holes_with_nuts[hole_i][1], lowest_z])
      cylinder(d=4, h=2.6, center=false);
    }
    
    translate([start_x + 296, start_y - 24, -pcb_depth])
    rotate([0, 90, 0])
    cylinder(d=8, h=10, center=true);
    
  }
  
      for (hole_i = [0:len(screw_holes)-1]) {
        translate([screw_holes[hole_i][0],-screw_holes[hole_i][1], lowest_z + 2.6])
        cylinder(d=4, h=0.2);
      } 
      for (hole_i = [0:len(center_screw_holes)-1]) {
        translate([center_screw_holes[hole_i][0],-center_screw_holes[hole_i][1], lowest_z + 2.6])
        cylinder(d=4, h=0.2);
      }
      for (hole_i = [0:len(screw_holes_with_nuts)-1]) {
        translate([screw_holes_with_nuts[hole_i][0],-screw_holes_with_nuts[hole_i][1], lowest_z + 2.6])
        cylinder(d=4, h=0.2);
      }      
}
}


union() {
  // OLEDHolder();
  // ScrewHoleWithNut();
  
  // TopPlate();

  // color("lightblue") SideDiffuser();
  
  color("teal") BottomPlateOnePiece();
}

