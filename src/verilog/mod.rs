use crate::sim::compile_to_packets;
use std::fs;
use std::path::Path;

pub struct VerilogBundle {
    pub rtl_code: String,
    pub tb_code: String,
}

pub fn generate_verilog(grid_size: usize, expr: &str) -> VerilogBundle {
    let tile_rtl = r#"// ============================================================================
// Frostbite Graph Core - Synthesizable RTL with Work-Stealing Arbiter
// Generated automatically by Frostbite Graph Core Simulator
// ============================================================================
`timescale 1ns / 1ps

module frostbite_tile #(
    parameter int TILE_X  = 0,
    parameter int TILE_Y  = 0,
    parameter int DATA_W  = 32,
    parameter int FIFO_D  = 4
)(
    input  logic              clk,
    input  logic              rst_n,
    input  logic              in_valid,
    input  logic [2:0]        in_op,      // 0:ADD, 1:SUB, 2:MUL, 3:DIV
    input  logic [DATA_W-1:0] in_arg_a,
    input  logic [DATA_W-1:0] in_arg_b,
    output logic              can_be_stolen,
    input  logic              steal_take,
    output logic [2:0]        export_op,
    output logic [DATA_W-1:0] export_a,
    output logic [DATA_W-1:0] export_b,
    input  logic [3:0]        neighbor_avail,
    output logic [3:0]        steal_req,
    input  logic              steal_grant,
    input  logic [2:0]        steal_op,
    input  logic [DATA_W-1:0] steal_arg_a,
    input  logic [DATA_W-1:0] steal_arg_b,
    output logic              out_valid,
    output logic [DATA_W-1:0] out_result,
    output logic              is_sleeping
);

    logic [2:0]        fifo_op [0:FIFO_D-1];
    logic [DATA_W-1:0] fifo_a  [0:FIFO_D-1];
    logic [DATA_W-1:0] fifo_b  [0:FIFO_D-1];
    logic [$clog2(FIFO_D):0] count;

    assign is_sleeping   = (count == 0) && !in_valid && !steal_grant;
    assign can_be_stolen = (count > 1);
    assign export_op     = (count > 0) ? fifo_op[count - 1] : 3'd0;
    assign export_a      = (count > 0) ? fifo_a[count - 1]  : '0;
    assign export_b      = (count > 0) ? fifo_b[count - 1]  : '0;

        always @* begin
        steal_req = 4'b0000;
        if (count == 0 && !in_valid) begin
            if (neighbor_avail[0])      steal_req = 4'b0001; // North
            else if (neighbor_avail[1]) steal_req = 4'b0010; // South
            else if (neighbor_avail[2]) steal_req = 4'b0100; // East
            else if (neighbor_avail[3]) steal_req = 4'b1000; // West
        end
    end

    always_ff @(posedge clk or negedge rst_n) begin
        if (!rst_n) begin
            count      <= '0;
            out_valid  <= 1'b0;
            out_result <= '0;
            for (int j = 0; j < FIFO_D; j++) begin
                fifo_op[j] <= 3'd0;
                fifo_a[j]  <= '0;
                fifo_b[j]  <= '0;
            end
        end else begin
            logic [$clog2(FIFO_D):0] next_cnt;
            next_cnt  = count;
            out_valid <= 1'b0;

            if (next_cnt > 0) begin
                out_valid <= 1'b1;
                case (fifo_op[0])
                    3'd0: out_result <= fifo_a[0] + fifo_b[0];
                    3'd1: out_result <= fifo_a[0] - fifo_b[0];
                    3'd2: out_result <= fifo_a[0] * fifo_b[0];
                    3'd3: out_result <= (fifo_b[0] != 0) ? (fifo_a[0] / fifo_b[0]) : '0;
                    default: out_result <= fifo_a[0];
                endcase
                for (int i = 0; i < FIFO_D - 1; i++) begin
                    fifo_op[i] <= fifo_op[i+1];
                    fifo_a[i]  <= fifo_a[i+1];
                    fifo_b[i]  <= fifo_b[i+1];
                end
                next_cnt = next_cnt - 1;
            end

            if (steal_take && next_cnt > 0) begin
                next_cnt = next_cnt - 1;
            end

            if (in_valid && next_cnt < FIFO_D) begin
                fifo_op[next_cnt] <= in_op;
                fifo_a[next_cnt]  <= in_arg_a;
                fifo_b[next_cnt]  <= in_arg_b;
                next_cnt = next_cnt + 1;
            end else if (steal_grant && next_cnt < FIFO_D) begin
                fifo_op[next_cnt] <= steal_op;
                fifo_a[next_cnt]  <= steal_arg_a;
                fifo_b[next_cnt]  <= steal_arg_b;
                next_cnt = next_cnt + 1;
            end

            count <= next_cnt;
        end
    end
endmodule
"#;

    let grid_rtl = format!(
        r#"
module frostbite_grid #(
    parameter int GRID_N = {n},
    parameter int DATA_W = 32
)(
    input  logic                     clk,
    input  logic                     rst_n,
    input  logic                     inject_valid,
    input  logic [2:0]               inject_op,
    input  logic [DATA_W-1:0]        inject_a,
    input  logic [DATA_W-1:0]        inject_b,
    output logic                     any_valid,
    output logic [DATA_W-1:0]        last_result,
    output logic [GRID_N*GRID_N-1:0] sleep_mask,
    output logic [GRID_N*GRID_N-1:0] steal_events
);

    localparam int TOTAL = GRID_N * GRID_N;

    logic              can_steal  [0:TOTAL-1];
    logic              steal_take [0:TOTAL-1];
    logic [2:0]        exp_op     [0:TOTAL-1];
    logic [DATA_W-1:0] exp_a      [0:TOTAL-1];
    logic [DATA_W-1:0] exp_b      [0:TOTAL-1];

    logic [3:0]        n_avail    [0:TOTAL-1];
    logic [3:0]        s_req      [0:TOTAL-1];
    logic              s_grant    [0:TOTAL-1];
    logic [2:0]        s_op       [0:TOTAL-1];
    logic [DATA_W-1:0] s_a        [0:TOTAL-1];
    logic [DATA_W-1:0] s_b        [0:TOTAL-1];

    logic              t_valid    [0:TOTAL-1];
    logic [DATA_W-1:0] t_res      [0:TOTAL-1];

    // МЕЖПЛИТОЧНЫЙ АРБИТР WORK-STEALING (совместимый с Icarus Verilog)
        always @* begin
        for (int i = 0; i < TOTAL; i++) begin
            steal_take[i]   = 1'b0;
            s_grant[i]      = 1'b0;
            s_op[i]         = 3'd0;
            s_a[i]          = '0;
            s_b[i]          = '0;
            steal_events[i] = 1'b0;
        end

        for (int y = 0; y < GRID_N; y++) begin
            for (int x = 0; x < GRID_N; x++) begin
                int id, target;
                logic [3:0] req_bits;
                id       = y * GRID_N + x;
                req_bits = s_req[id];
                target   = -1;

                if (req_bits[0] && y > 0)               target = (y - 1) * GRID_N + x;
                else if (req_bits[1] && y < GRID_N - 1) target = (y + 1) * GRID_N + x;
                else if (req_bits[2] && x < GRID_N - 1) target = y * GRID_N + (x + 1);
                else if (req_bits[3] && x > 0)          target = y * GRID_N + (x - 1);

                if (target >= 0 && can_steal[target] && !steal_take[target]) begin
                    steal_take[target] = 1'b1;
                    s_grant[id]        = 1'b1;
                    s_op[id]           = exp_op[target];
                    s_a[id]            = exp_a[target];
                    s_b[id]            = exp_b[target];
                    steal_events[id]   = 1'b1;
                end
            end
        end
    end

    genvar gx, gy;
    generate
        for (gy = 0; gy < GRID_N; gy++) begin : row
            for (gx = 0; gx < GRID_N; gx++) begin : col
                localparam int ID = gy * GRID_N + gx;

                if (gy > 0) begin : has_n
                    assign n_avail[ID][0] = can_steal[(gy-1)*GRID_N + gx];
                end else begin : no_n
                    assign n_avail[ID][0] = 1'b0;
                end

                if (gy < GRID_N - 1) begin : has_s
                    assign n_avail[ID][1] = can_steal[(gy+1)*GRID_N + gx];
                end else begin : no_s
                    assign n_avail[ID][1] = 1'b0;
                end

                if (gx < GRID_N - 1) begin : has_e
                    assign n_avail[ID][2] = can_steal[gy*GRID_N + (gx+1)];
                end else begin : no_e
                    assign n_avail[ID][2] = 1'b0;
                end

                if (gx > 0) begin : has_w
                    assign n_avail[ID][3] = can_steal[gy*GRID_N + (gx-1)];
                end else begin : no_w
                    assign n_avail[ID][3] = 1'b0;
                end

                frostbite_tile #(.TILE_X(gx), .TILE_Y(gy), .DATA_W(DATA_W)) u_tile (
                    .clk           (clk),
                    .rst_n         (rst_n),
                    .in_valid      ((ID == 0) ? inject_valid : 1'b0),
                    .in_op         ((ID == 0) ? inject_op    : 3'd0),
                    .in_arg_a      ((ID == 0) ? inject_a     : '0),
                    .in_arg_b      ((ID == 0) ? inject_b     : '0),
                    .can_be_stolen (can_steal[ID]),
                    .steal_take    (steal_take[ID]),
                    .export_op     (exp_op[ID]),
                    .export_a      (exp_a[ID]),
                    .export_b      (exp_b[ID]),
                    .neighbor_avail(n_avail[ID]),
                    .steal_req     (s_req[ID]),
                    .steal_grant   (s_grant[ID]),
                    .steal_op      (s_op[ID]),
                    .steal_arg_a   (s_a[ID]),
                    .steal_arg_b   (s_b[ID]),
                    .out_valid     (t_valid[ID]),
                    .out_result    (t_res[ID]),
                    .is_sleeping   (sleep_mask[ID])
                );
            end
        end
    endgenerate

    always_comb begin
        any_valid   = 1'b0;
        last_result = '0;
        for (int i = 0; i < TOTAL; i++) begin
            if (t_valid[i]) begin
                any_valid   = 1'b1;
                last_result = t_res[i];
            end
        end
    end
endmodule
"#,
        n = grid_size
    );


    let rtl_code = format!("{tile_rtl}\n{grid_rtl}");

    // Компилируем пользовательское выражение в последовательность пакетов для тестбенча
    let (stimulus_sv, expected_summary) = match compile_to_packets(expr) {
        Ok((packets, final_val)) if !packets.is_empty() => {
            let mut lines = String::new();
            for (idx, pkt) in packets.iter().enumerate() {
                lines.push_str(&format!(
                    "        // Packet #{}: {}({}, {}) -> expected {}\n        @(posedge clk);\n        inject_valid <= 1'b1;\n        inject_op    <= 3'd{};\n        inject_a     <= 32'sd{};\n        inject_b     <= 32'sd{};\n",
                    idx + 1,
                    pkt.op_name,
                    pkt.arg_a,
                    pkt.arg_b,
                    pkt.expected,
                    pkt.op_code,
                    pkt.arg_a,
                    pkt.arg_b
                ));
            }
            lines.push_str("        @(posedge clk);\n        inject_valid <= 1'b0;\n");
            (lines, format!("{} (total packets: {})", final_val, packets.len()))
        }
        _ => (
            "        @(posedge clk);\n        inject_valid <= 1'b1;\n        inject_op    <= 3'd2;\n        inject_a     <= 32'sd15;\n        inject_b     <= 32'sd4;\n        @(posedge clk);\n        inject_valid <= 1'b0;\n".to_string(),
            "60 (fallback)".to_string(),
        ),
    };

    let tb_code = format!(
        r#"// ============================================================================
// Auto-generated Testbench for Frostbite Graph Core ({n}x{n})
// Compiled expression: {expr}
// Expected final value: {exp_sum}
// Run with: iverilog -g2012 -o sim.out frostbite_grid.sv frostbite_tb.sv && vvp sim.out
// ============================================================================
`timescale 1ns / 1ps

module frostbite_tb;

    localparam int GRID_N = {n};
    localparam int DATA_W = 32;

    logic                     clk = 0;
    logic                     rst_n = 0;
    logic                     inject_valid = 0;
    logic [2:0]               inject_op = 0;
    logic [DATA_W-1:0]        inject_a = 0;
    logic [DATA_W-1:0]        inject_b = 0;
    logic                     any_valid;
    logic [DATA_W-1:0]        last_result;
    logic [GRID_N*GRID_N-1:0] sleep_mask;
    logic [GRID_N*GRID_N-1:0] steal_events;

    always #5 clk = ~clk;

    frostbite_grid #(.GRID_N(GRID_N), .DATA_W(DATA_W)) dut (
        .clk         (clk),
        .rst_n       (rst_n),
        .inject_valid(inject_valid),
        .inject_op   (inject_op),
        .inject_a    (inject_a),
        .inject_b    (inject_b),
        .any_valid   (any_valid),
        .last_result (last_result),
        .sleep_mask  (sleep_mask),
        .steal_events(steal_events)
    );

    always @(posedge clk) begin
        if (rst_n && (|steal_events))
            $display("[T=%0t] WORK-STEALING event! Steal mask = %b", $time, steal_events);
        if (rst_n && any_valid)
            $display("[T=%0t] REDUCTION output = %0d | Sleep mask = %b", $time, $signed(last_result), sleep_mask);
    end

    initial begin
        $dumpfile("frostbite_wave.vcd");
        $dumpvars(0, frostbite_tb);

        #20 rst_n = 1;
{stimulus}
        #100;
        $display("[FROSTBITE RTL] Simulation finished. Expected final: {exp_sum}");
        $finish;
    end

endmodule
"#,
        n = grid_size,
        expr = expr,
        exp_sum = expected_summary,
        stimulus = stimulus_sv
    );

    VerilogBundle { rtl_code, tb_code }
}

pub fn export_verilog_files(dir: &str, bundle: &VerilogBundle) -> Result<String, String> {
    let out_dir = Path::new(dir);
    fs::create_dir_all(out_dir).map_err(|e| format!("Ошибка создания папки: {e}"))?;

    let rtl_path = out_dir.join("frostbite_grid.sv");
    let tb_path = out_dir.join("frostbite_tb.sv");

    fs::write(&rtl_path, &bundle.rtl_code).map_err(|e| format!("Ошибка записи RTL: {e}"))?;
    fs::write(&tb_path, &bundle.tb_code).map_err(|e| format!("Ошибка записи TB: {e}"))?;

    Ok(format!(
        "Сохранено в {}/ (frostbite_grid.sv + frostbite_tb.sv)",
        dir
    ))
}

