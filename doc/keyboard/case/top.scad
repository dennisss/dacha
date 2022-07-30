
start_x = 100;
start_y = -100;

outer_width = 365;
outer_height = 130;

unit_dim = 19.05;
key_hole_dim = 14;
key_hole_pad = 0.1;

// 
plate_depth = 1.6;

// Distance from the top of the plate to the pcb (which is below the plate)
// According to Cherry MX the distance is 5mm but we add some space for foam pads (which are usually 0.5mm)
plate_to_pcb_depth = 5.2;

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
hex_nut_height = 2;

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
  translate([oled_header_x - 1.5 + (oled_outer_width / 2), -oled_header_y, 0])
  cube([oled_display_width + oled_padding, 12.2 + oled_padding, 100], center=true);
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

module PlateWall() {
  translate([start_x, start_y, -pcb_depth])
  translate([outer_width / 2, -outer_height / 2, (plate_to_pcb_depth + pcb_depth) / 2])
  difference() {
    cube([outer_width, outer_height, plate_to_pcb_depth + pcb_depth], center=true);
    cube([outer_width - 2*outer_wall_width, outer_height - 2*outer_wall_width, 100], center=true);
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

module ScrewHole() {
  translate([0, 0, plate_to_pcb_depth / 2])
  // 4.7 is 3 0.45mm walls
  difference() {
    cylinder(h=plate_to_pcb_depth, d=4.7, center=true);
    cylinder(h=100, d=2, center=true);
  }
}



module ScrewHoleWithNut() {
  difference() {
    translate([0, 0, plate_to_pcb_depth / 2])
      cube([8, 8, plate_to_pcb_depth], center = true);
    cylinder(h=100, d=2, center=true);
    
    rotate(-90)
    translate([0, 0, (-hex_nut_height / 2) + (plate_to_pcb_depth - plate_depth)])
    linear_extrude(height = hex_nut_height, center = true)
    union() {
      Hexagon(hex_nut_radius);
      HexagonProtrusion(hex_nut_radius, 10);
    }    
  }
}

// The base of one stabilizer is 20mm high (the amount of space touching the PCB)
// But, further up, the part that will actually need to protrude out of the plate is only 11.2mm high.
module StabHole() {
  cube([7, 11.2, 100], center=true);
}
module StabClearance() {
  cube([7, 20, 100], center=true);
}

module StabWireClearance(data) {
  translate([data[0], -data[2], 0])
  cube([data[1] - data[0], 3*stabilizer_wire_width, 100], center=false);
}

module Main() {
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
    
    PlateWall();
  }
}

keyboard_half_width = outer_width / 2;
keyboard_center_x = start_x + keyboard_half_width;

difference() {
  Main();
  
  /*
  // Comment or uncomment this line to generate the left and right sides.
  translate([keyboard_half_width, 0, 0])
  translate([start_x, start_y - outer_height - 50, -50])
  cube([keyboard_half_width, outer_height + 100, 100], center=false);
  */
}

