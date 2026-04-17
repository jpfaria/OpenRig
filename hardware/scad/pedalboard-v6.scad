// ============================================
// OpenRig Pedalboard Controller v6
// 4 pieces: LT, LB, RT, RB
// REINFORCED footswitch zones
// 7" Raspberry Pi display (194x110mm board)
// ============================================

// === Enclosure ===
length          = 560;
width           = 280;
h_front         = 30;
h_back          = 60;
wall            = 3;
floor_t         = 3;
top_thick       = 4;        // thicker top panel for stomp strength

// Components
fs_hole         = 12.5;     // 12mm momentary + 0.5 clearance
pot_hole        = 7.5;
spacing         = 52;
cols            = 10;

// Row Y positions
row1_y          = 45;
row2_y          = 100;
row3_y          = 245;

// === 7" Display (Raspberry Pi Official 7" Touchscreen) ===
// Board: 194 x 110mm
// Active area: 154.08 x 85.92mm
// Mounting holes: 4x M2.5 on the board corners
disp_active_w   = 157.45;   // mm - visible cutout (from datasheet)      // mm - visible cutout width
disp_active_h   = 89.90;    // mm - visible cutout       // mm - visible cutout height
disp_board_w    = 166.50;   // mm - PCB outer dimension      // mm - full PCB width
disp_board_h    = 120.03;   // mm - PCB outer dimension      // mm - full PCB height
disp_bezel_w    = 166.50;   // mm - bezel recess matches PCB      // mm - bezel opening (slightly larger than active)
disp_bezel_h    = 120.03;   // mm - bezel recess matches PCB
disp_bezel_depth = 2;       // mm - recess for bezel/glass
disp_center_y   = 170;
// M2.5 mounting (on PCB corners, 4 holes)
disp_mount_d    = 2.7;      // M2.5 clearance
disp_mount_x_spacing = 156.63; // mm - from datasheet // mm between mount holes X
disp_mount_y_spacing = 115.04; // mm - from datasheet  // mm between mount holes Y
// === Orange Pi 5B ===
// PCB: ~90 x 64mm, RK3588S, same form factor as OPi 5
// Orange Pi 5B - rotated so port edge faces back panel
opi_pcb_w       = 64;       // mm - board width in X (short side)
opi_pcb_d       = 90;       // mm - board depth in Y (long/port side)
opi_mount_w     = 56;       // mm - mounting hole spacing X
opi_mount_d     = 82;       // mm - mounting hole spacing Y
opi_mount_hole  = 2.7;      // M2.5 clearance
opi_standoff_d  = 6;        // mm - standoff outer diameter
opi_standoff_h  = 8;        // mm - standoff height
// Position: right half, port edge flush with back panel inner wall
opi_x           = length/2 + 50;           // X center
opi_y           = width - wall - opi_pcb_d/2;  // Y center (ports at back)

// OPi 5B port cutouts in back panel (positions relative to opi_x)
// Measured from left edge of board along 90mm port side:
// USB 2.0 stacked: ~7mm from left, 14mm wide, 14mm tall
opi_usb2_offset   = -30;    // mm from board center X
opi_usb2_w        = 14;
opi_usb2_h        = 14;
// HDMI: ~28mm from left, 15mm wide, 6mm tall
opi_hdmi_offset   = -10;
opi_hdmi_w        = 16;
opi_hdmi_h        = 7;
// Gigabit Ethernet: ~52mm from left, 16mm wide, 14mm tall
opi_eth_offset    = 12;
opi_eth_w         = 17;
opi_eth_h         = 14;
// USB 3.0 (other side, but accessible from back too): ~70mm from left
opi_usb3_offset   = 25;
opi_usb3_w        = 8;
opi_usb3_h        = 5;
// Type-C power (other edge) - internal, no cutout needed
// 3.5mm audio (other edge) - internal, connected to audio interface


// Connectors
usb_w = 12; usb_h = 7;
barrel_d = 8; jack_d = 10; midi_d = 16;

// Feet
foot_d = 15; foot_depth = 2;

// === TOP/BOTTOM JOINT ===
lip_inset       = 1.5;
screw_d         = 3.2;      // M3 clearance
screw_tap_d     = 2.5;      // M3 tap for self-threading into plastic
screw_head_d    = 6;
screw_head_depth = 2.5;
screw_spacing   = 60;

// === LEFT/RIGHT SPLIT ===
split_x         = length / 2;

// === FOOTSWITCH REINFORCEMENT ===
fs_reinforce_d    = 22;     // mm - reinforcement ring outer diameter
fs_reinforce_h    = 5;      // mm - extra thickness ring under top panel
fs_pillar_d       = 16;     // mm - support pillar diameter from floor
fs_rib_w          = 3;      // mm - connecting rib width between pillars
fs_rib_h          = 10;     // mm - rib height

$fn = 40;

// === Helpers ===
function hz(y) = h_front + (h_back - h_front) * (y / width);

// === Which part to render ===
// "lt" "lb" "rt" "rb" "assembled" "exploded"
part = "assembled";

// =============================================
// BOTTOM TRAY
// =============================================
module bottom_tray() {
    x0 = (length - (cols-1) * spacing) / 2;
    cx = length / 2;
    
    difference() {
        union() {
            // --- Floor plate ---
            polyhedron(
                points = [
                    [0,0,0],[length,0,0],[length,width,0],[0,width,0],
                    [0,0,floor_t],[length,0,floor_t],[length,width,floor_t],[0,width,floor_t]
                ],
                faces = [[0,1,2,3],[4,7,6,5],[0,4,5,1],[2,6,7,3],[0,3,7,4],[1,5,6,2]]
            );
            
            // --- Perimeter walls ---
            // Front wall
            polyhedron(
                points = [
                    [0,0,0],[length,0,0],[length,wall,0],[0,wall,0],
                    [0,0,h_front-top_thick],[length,0,h_front-top_thick],
                    [length,wall,h_front-top_thick],[0,wall,h_front-top_thick]
                ],
                faces = [[0,1,2,3],[4,7,6,5],[0,4,5,1],[2,6,7,3],[0,3,7,4],[1,5,6,2]]
            );
            // Back wall
            polyhedron(
                points = [
                    [0,width-wall,0],[length,width-wall,0],[length,width,0],[0,width,0],
                    [0,width-wall,h_back-top_thick],[length,width-wall,h_back-top_thick],
                    [length,width,h_back-top_thick],[0,width,h_back-top_thick]
                ],
                faces = [[0,1,2,3],[4,7,6,5],[0,4,5,1],[2,6,7,3],[0,3,7,4],[1,5,6,2]]
            );
            // Left wall
            polyhedron(
                points = [
                    [0,0,0],[wall,0,0],[wall,width,0],[0,width,0],
                    [0,0,h_front-top_thick],[wall,0,h_front-top_thick],
                    [wall,width,h_back-top_thick],[0,width,h_back-top_thick]
                ],
                faces = [[0,1,2,3],[4,7,6,5],[0,4,5,1],[2,6,7,3],[0,3,7,4],[1,5,6,2]]
            );
            // Right wall
            polyhedron(
                points = [
                    [length-wall,0,0],[length,0,0],[length,width,0],[length-wall,width,0],
                    [length-wall,0,h_front-top_thick],[length,0,h_front-top_thick],
                    [length,width,h_back-top_thick],[length-wall,width,h_back-top_thick]
                ],
                faces = [[0,1,2,3],[4,7,6,5],[0,4,5,1],[2,6,7,3],[0,3,7,4],[1,5,6,2]]
            );
            
            // --- Inner lip ledges ---
            for (side = [[wall, wall, lip_inset, length-2*wall],           // front lip
                         [wall, width-wall-lip_inset, lip_inset, length-2*wall]]) // back lip
                translate([side[0], side[1], floor_t])
                    cube([side[3], side[2], max(1, h_front - top_thick - floor_t)]);
            
            // --- FOOTSWITCH SUPPORT PILLARS ---
            // Each footswitch gets a pillar from floor to just under top panel
            for (row = [0, 1]) {
                ry = (row == 0) ? row1_y : row2_y;
                for (i = [0 : cols-1]) {
                    fx = x0 + i * spacing;
                    pillar_top = hz(ry) - top_thick;
                    if (pillar_top > floor_t + 2) {
                        // Main pillar
                        translate([fx, ry, floor_t])
                            cylinder(h=pillar_top - floor_t, d=fs_pillar_d, $fn=24);
                    }
                }
                // Connecting ribs between pillars in each row
                for (i = [0 : cols-2]) {
                    fx1 = x0 + i * spacing;
                    fx2 = x0 + (i+1) * spacing;
                    rib_top = hz(ry) - top_thick;
                    translate([fx1 + fs_pillar_d/2, ry - fs_rib_w/2, floor_t])
                        cube([fx2 - fx1 - fs_pillar_d, fs_rib_w, min(fs_rib_h, rib_top - floor_t)]);
                }
            }
            
            // Cross ribs connecting row1 to row2 (every other pillar)
            for (i = [0 : 2 : cols-1]) {
                fx = x0 + i * spacing;
                rib_top = min(hz(row1_y), hz(row2_y)) - top_thick;
                translate([fx - fs_rib_w/2, row1_y, floor_t])
                    cube([fs_rib_w, row2_y - row1_y, min(fs_rib_h, rib_top - floor_t)]);
            }
            
            // --- Perimeter screw posts ---
            for (x = [30 : screw_spacing : length - 20]) {
                // Front
                translate([x, wall + lip_inset/2, floor_t])
                    cylinder(h=h_front - top_thick - floor_t, d=screw_head_d + 3, $fn=20);
                // Back
                translate([x, width - wall - lip_inset/2, floor_t])
                    cylinder(h=h_back - top_thick - floor_t, d=screw_head_d + 3, $fn=20);
            }
            for (y = [50 : screw_spacing : width - 30]) {
                translate([wall + lip_inset/2, y, floor_t])
                    cylinder(h=hz(y) - top_thick - floor_t, d=screw_head_d + 3, $fn=20);
                translate([length - wall - lip_inset/2, y, floor_t])
                    cylinder(h=hz(y) - top_thick - floor_t, d=screw_head_d + 3, $fn=20);
            }
            
            // --- Display mounting posts ---
            for (dx = [-1, 1]) for (dy = [-1, 1]) {
                mx = cx + dx * disp_mount_x_spacing/2;
                my = disp_center_y + dy * disp_mount_y_spacing/2;
                z_top = hz(my) - top_thick;
                if (z_top > floor_t + 2)
                translate([mx, my, floor_t])
                    difference() {
                        cylinder(h=z_top - floor_t, d=8, $fn=20);
                        cylinder(h=z_top - floor_t + 1, d=disp_mount_d, $fn=16);
                    }
            }
        }
        
        // --- Drill screw tap holes in posts ---
        for (x = [30 : screw_spacing : length - 20]) {
            translate([x, wall + lip_inset/2, -0.1])
                cylinder(h=h_front + 1, d=screw_tap_d, $fn=16);
            translate([x, width - wall - lip_inset/2, -0.1])
                cylinder(h=h_back + 1, d=screw_tap_d, $fn=16);
        }
        for (y = [50 : screw_spacing : width - 30]) {
            translate([wall + lip_inset/2, y, -0.1])
                cylinder(h=hz(y) + 1, d=screw_tap_d, $fn=16);
            translate([length - wall - lip_inset/2, y, -0.1])
                cylinder(h=hz(y) + 1, d=screw_tap_d, $fn=16);
        }
        
        // --- Hollow out pillar centers (so screw from top can reach) ---
        for (row = [0, 1]) {
            ry = (row == 0) ? row1_y : row2_y;
            for (i = [0 : cols-1]) {
                fx = x0 + i * spacing;
                // Keep pillar solid — no drilling. The stomp force 
                // transfers through the solid pillar to the floor.
            }
        }
        
        // --- Back panel cutouts ---
        // USB-C
        translate([cx - usb_w/2, width-wall-0.5, h_back/2 - usb_h/2])
            cube([usb_w, wall+1, usb_h]);
        // Power barrel
        translate([40, width, h_back/2]) rotate([90,0,0])
            cylinder(h=wall+1, d=barrel_d, center=true, $fn=20);
        // MIDI
        translate([80, width, h_back/2]) rotate([90,0,0])
            cylinder(h=wall+1, d=midi_d, center=true, $fn=20);
        // Ethernet (panel mount)
        translate([cx + 40 - 8, width-wall-0.5, h_back/2 - 7])
            cube([16, wall+1, 14]);
        
        // === Orange Pi 5B port cutouts (back panel) ===
        // USB 2.0 stacked
        translate([opi_x + opi_usb2_offset - opi_usb2_w/2, width-wall-0.5, 
                   floor_t + opi_standoff_h + 2])
            cube([opi_usb2_w, wall+1, opi_usb2_h]);
        // HDMI
        translate([opi_x + opi_hdmi_offset - opi_hdmi_w/2, width-wall-0.5,
                   floor_t + opi_standoff_h + 3])
            cube([opi_hdmi_w, wall+1, opi_hdmi_h]);
        // Gigabit Ethernet (OPi)
        translate([opi_x + opi_eth_offset - opi_eth_w/2, width-wall-0.5,
                   floor_t + opi_standoff_h + 2])
            cube([opi_eth_w, wall+1, opi_eth_h]);
        // USB 3.0
        translate([opi_x + opi_usb3_offset - opi_usb3_w/2, width-wall-0.5,
                   floor_t + opi_standoff_h + 3])
            cube([opi_usb3_w, wall+1, opi_usb3_h]);
        // 1/4" jacks
        for (i = [0:3])
            translate([length-40-i*25, width, h_back/2]) rotate([90,0,0])
                cylinder(h=wall+1, d=jack_d, center=true, $fn=20);
        
        // Side vents
        for (i = [0:9]) {
            translate([-0.5, 50+i*12, h_front/2+3])
                cube([wall+1, 1.5, 10]);
            translate([length-wall-0.5, 50+i*12, h_front/2+3])
                cube([wall+1, 1.5, 10]);
        }
        

        // Orange Pi ventilation (bottom, under the board)
        for (i = [0:5]) {
            translate([opi_x - 25 + i*10, opi_y - 20, -0.1])
                cylinder(h=floor_t + 0.2, d=3, $fn=12);
            translate([opi_x - 25 + i*10, opi_y + 20, -0.1])
                cylinder(h=floor_t + 0.2, d=3, $fn=12);
        }
        
        // DSI cable routing slot (between OPi and display)
        translate([opi_x - 10, disp_center_y + disp_active_h/2 + 5, floor_t + opi_standoff_h - 2])
            cube([20, opi_y - disp_center_y - disp_active_h/2 - 10, 5]);

        // Rubber feet
        for (pos = [[30,30],[length-30,30],[30,width-30],
                     [length-30,width-30],[cx,30],[cx,width-30]])
            translate([pos[0], pos[1], -0.01])
                cylinder(h=foot_depth, d=foot_d, $fn=30);
    }
}

// =============================================
// TOP PANEL (angled slab, reinforced FS zones)
// =============================================
module top_panel() {
    x0 = (length - (cols-1) * spacing) / 2;
    cx = length / 2;
    
    difference() {
        union() {
            // Main angled slab
            polyhedron(
                points = [
                    [wall+lip_inset, wall+lip_inset, h_front-top_thick],
                    [length-wall-lip_inset, wall+lip_inset, h_front-top_thick],
                    [length-wall-lip_inset, width-wall-lip_inset, h_back-top_thick],
                    [wall+lip_inset, width-wall-lip_inset, h_back-top_thick],
                    [0, 0, h_front],
                    [length, 0, h_front],
                    [length, width, h_back],
                    [0, width, h_back],
                ],
                faces = [[0,1,2,3],[4,7,6,5],[0,4,5,1],[2,6,7,3],[0,3,7,4],[1,5,6,2]]
            );
            
            // === FOOTSWITCH REINFORCEMENT RINGS ===
            // Thick rings on the underside around each FS hole
            for (row = [0, 1]) {
                ry = (row == 0) ? row1_y : row2_y;
                for (i = [0 : cols-1]) {
                    fx = x0 + i * spacing;
                    z_bottom = hz(ry) - top_thick;
                    // Ring hangs below the panel
                    translate([fx, ry, z_bottom - fs_reinforce_h])
                        difference() {
                            cylinder(h=fs_reinforce_h, d=fs_reinforce_d, $fn=24);
                            translate([0, 0, -0.1])
                                cylinder(h=fs_reinforce_h + 0.2, d=fs_hole + 0.5, $fn=24);
                        }
                }
            }
        }
        
        // === Component holes ===
        // FS Row 1
        for (i = [0:cols-1]) {
            x = x0 + i*spacing;
            translate([x, row1_y, hz(row1_y) - top_thick - fs_reinforce_h - 1])
                cylinder(h=top_thick + fs_reinforce_h + 2, d=fs_hole);
        }
        // FS Row 2
        for (i = [0:cols-1]) {
            x = x0 + i*spacing;
            translate([x, row2_y, hz(row2_y) - top_thick - fs_reinforce_h - 1])
                cylinder(h=top_thick + fs_reinforce_h + 2, d=fs_hole);
        }
        // Pots
        for (i = [0:cols-1]) {
            x = x0 + i*spacing;
            translate([x, row3_y, hz(row3_y) - top_thick - 1])
                cylinder(h=top_thick + 2, d=pot_hole);
        }
        
        // === 7" Display cutout ===
        // Active area cutout
        translate([cx - disp_active_w/2, disp_center_y - disp_active_h/2, 
                   hz(disp_center_y) - top_thick - 1])
            cube([disp_active_w, disp_active_h, top_thick + 2]);
        // Bezel recess
        translate([cx - disp_bezel_w/2, disp_center_y - disp_bezel_h/2,
                   hz(disp_center_y) + top_thick - disp_bezel_depth])
            cube([disp_bezel_w, disp_bezel_h, disp_bezel_depth + 1]);
        // Mount holes (M2.5)
        for (dx = [-1, 1]) for (dy = [-1, 1]) {
            mx = cx + dx * disp_mount_x_spacing/2;
            my = disp_center_y + dy * disp_mount_y_spacing/2;
            translate([mx, my, hz(my) - top_thick - 1])
                cylinder(h=top_thick + 2, d=disp_mount_d, $fn=16);
        }
        
        // === Perimeter screw holes (countersunk) ===
        for (x = [30 : screw_spacing : length - 20]) {
            for (y_pos = [wall + lip_inset/2, width - wall - lip_inset/2]) {
                z = hz(y_pos) - top_thick;
                translate([x, y_pos, z - 0.1]) {
                    cylinder(h=top_thick + 1, d=screw_d, $fn=16);
                    translate([0, 0, top_thick - screw_head_depth])
                        cylinder(h=screw_head_depth + 1, d=screw_head_d, $fn=16);
                }
            }
        }
        for (y = [50 : screw_spacing : width - 30]) {
            for (x_pos = [wall + lip_inset/2, length - wall - lip_inset/2]) {
                z = hz(y) - top_thick;
                translate([x_pos, y, z - 0.1]) {
                    cylinder(h=top_thick + 1, d=screw_d, $fn=16);
                    translate([0, 0, top_thick - screw_head_depth])
                        cylinder(h=screw_head_depth + 1, d=screw_head_d, $fn=16);
                }
            }
        }
    }
}

// === SPLITTING ===
module cut_left() {
    translate([-1, -1, -1])
        cube([split_x + 1, width + 2, h_back + 30]);
}
module cut_right() {
    translate([split_x, -1, -1])
        cube([split_x + 1, width + 2, h_back + 30]);
}

// === RENDER ===
if (part == "lb") {
    intersection() { bottom_tray(); cut_left(); }
} else if (part == "rb") {
    translate([-split_x, 0, 0])
    intersection() { bottom_tray(); cut_right(); }
} else if (part == "lt") {
    intersection() { top_panel(); cut_left(); }
} else if (part == "rt") {
    translate([-split_x, 0, 0])
    intersection() { top_panel(); cut_right(); }
} else if (part == "assembled") {
    bottom_tray();
    top_panel();
} else if (part == "exploded") {
    translate([0, 0, 0])
        intersection() { bottom_tray(); cut_left(); }
    translate([0, 0, h_back + 20])
        intersection() { top_panel(); cut_left(); }
    translate([split_x + 40, 0, 0])
        translate([-split_x, 0, 0])
        intersection() { bottom_tray(); cut_right(); }
    translate([split_x + 40, 0, h_back + 20])
        translate([-split_x, 0, 0])
        intersection() { top_panel(); cut_right(); }
}
