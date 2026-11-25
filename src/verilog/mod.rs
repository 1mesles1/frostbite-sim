use std::fs;
use std::path::Path;

pub struct VerilogBundle {
    pub rtl_code: String,
    pub tb_code: String,
}

pub fn generate_verilog(grid_size: usize, expr: &str) -> VerilogBundle {
    let rtl_code = format!(
        r#"// ============================================================================
// Frostbite Graph Core - Synthesizable RTL (Grid {n}x{n})
// Generated automatically by Frostbite Graph Core Simulator
// ============================================================================
`timescale 1ns / 1ps

module frostbite_tile #(
    parameter int TILE_X = 0,
    parameter int TILE_Y = 0,
    parameter int DATA_W = 32
)(
    input  logic              clk,
    input  logic              rst_n,
    // Вход новой задачи (вброс узла редукции)
    input  logic              in_valid,
    input  logic [2:0]        in_op,      // 0:ADD, 1:SUB, 2:MUL, 3:FIB_STEP
    input  logic [DATA_W-1:0] in_arg_a,
    input  logic [DATA_W-1:0] in_arg_b,
    // Интерфейс Work-Stealing (соседи N, S, E, W)
    input  logic [3:0]        neighbor_busy,
    output logic [3:0]        steal_req,
    input  logic              steal_grant,
    input  logic [2:0]        steal_op,
    input  logic [DATA_W-1:0] steal_arg_a,
    input  logic [DATA_W-1:0] steal_arg_b,
    // Выход редуцированного значения и телеметрия сна
    output logic              out_valid,
    output logic [DATA_W-1:0] out_result,
    output logic              is_sleeping
);

    typedef enum logic [1:0] {{
        ST_SLEEP   = 2'b00,
        ST_COMPUTE = 2'b01,
        ST_STEAL   = 2'b10
    }} state_t;

    state_t state, next_state;
    logic [2:0]        reg_op;
    logic [DATA_W-1:0] reg_a, reg_b;

    // Уход в глубокий сон (Clock-gating hint) при отсутствии активных узлов
    assign is_sleeping = (state == ST_SLEEP);

    always_ff @(posedge clk or negedge rst_n) begin
        if (!rst_n) begin
            state      <= ST_SLEEP;
            reg_op     <= 3'd0;
            reg_a      <= '0;
            reg_b      <= '0;
            out_valid  <= 1'b0;
            out_result <= '0;
        end else begin
            out_valid <= 1'b0;
            if (in_valid) begin
                state  <= ST_COMPUTE;
                reg_op <= in_op;
                reg_a  <= in_arg_a;
                reg_b  <= in_arg_b;
            end else if (steal_grant) begin
                state  <= ST_STEAL;
                reg_op <= steal_op;
                reg_a  <= steal_arg_a;
                reg_b  <= steal_arg_b;
            end else if (state == ST_COMPUTE || state == ST_STEAL) begin
                // Локальная редукция активной пары Interaction Net за 1 такт
                out_valid <= 1'b1;
                case (reg_op)
                    3'd0: out_result <= reg_a + reg_b;          // ADD
                    3'd1: out_result <= reg_a - reg_b;          // SUB
                    3'd2: out_result <= reg_a * reg_b;          // MUL
                    3'd3: out_result <= (reg_a <= 1) ? reg_a : (reg_a - 1); // FIB step
                    default: out_result <= reg_a;
                endcase
                state <= ST_SLEEP;
            end
        end
    end

    // Логика запроса перехвата задач у перегруженных соседей без диспетчера
    always_comb begin
        steal_req = 4'b0000;
        if (state == ST_SLEEP && !in_valid) begin
            if (neighbor_busy[0])      steal_req = 4'b0001; // North
            else if (neighbor_busy[1]) steal_req = 4'b0010; // South
            else if (neighbor_busy[2]) steal_req = 4'b0100; // East
            else if (neighbor_busy[3]) steal_req = 4'b1000; // West
        end
    end

endmodule

module frostbite_grid #(
    parameter int GRID_N = {n},
    parameter int DATA_W = 32
)(
    input  logic              clk,
    input  logic              rst_n,
    input  logic              inject_valid,
    input  logic [2:0]        inject_op,
    input  logic [DATA_W-1:0] inject_a,
    input  logic [DATA_W-1:0] inject_b,
    output logic              root_done,
    output logic [DATA_W-1:0] root_result,
    output logic [GRID_N*GRID_N-1:0] sleep_mask
);

    logic [GRID_N*GRID_N-1:0] busy_vec;
    assign busy_vec = ~sleep_mask;

    genvar gx, gy;
    generate
        for (gy = 0; gy < GRID_N; gy++) begin : row
            for (gx = 0; gx < GRID_N; gx++) begin : col
                localparam int ID = gy * GRID_N + gx;
                logic [3:0] n_busy;
                assign n_busy[0] = (gy > 0)          ? busy_vec[(gy-1)*GRID_N + gx] : 1'b0;
                assign n_busy[1] = (gy < GRID_N - 1) ? busy_vec[(gy+1)*GRID_N + gx] : 1'b0;
                assign n_busy[2] = (gx < GRID_N - 1) ? busy_vec[gy*GRID_N + (gx+1)] : 1'b0;
                assign n_busy[3] = (gx > 0)          ? busy_vec[gy*GRID_N + (gx-1)] : 1'b0;

                frostbite_tile #(.TILE_X(gx), .TILE_Y(gy), .DATA_W(DATA_W)) u_tile (
                    .clk          (clk),
                    .rst_n        (rst_n),
                    .in_valid     ((ID == 0) ? inject_valid : 1'b0),
                    .in_op        ((ID == 0) ? inject_op    : 3'd0),
                    .in_arg_a     ((ID == 0) ? inject_a     : '0),
                    .in_arg_b     ((ID == 0) ? inject_b     : '0),
                    .neighbor_busy(n_busy),
                    .steal_req    (),
                    .steal_grant  (1'b0),
                    .steal_op     (3'd0),
                    .steal_arg_a  ('0),
                    .steal_arg_b  ('0),
                logic tile_valid;
                logic [DATA_W-1:0] tile_res;
                if (ID == 0) begin : gen_root
                    assign root_done   = tile_valid;
                    assign root_result = tile_res;
                end
                    .is_sleeping  (sleep_mask[ID])
                );
            end
        end
    endgenerate

endmodule
"#,
        n = grid_size
    );
    let tb_code = format!(
        r#"// ============================================================================
// Testbench for Frostbite Graph Core ({n}x{n})
// Expression context: {expr}
// Run with: iverilog -g2012 -o sim.out frostbite_grid.sv frostbite_tb.sv && vvp sim.out
// ============================================================================
`timescale 1ns / 1ps

module frostbite_tb;

    localparam int GRID_N = {n};
    localparam int DATA_W = 32;

    logic clk = 0;
    logic rst_n = 0;
    logic inject_valid = 0;
    logic [2:0] inject_op = 0;
    logic [DATA_W-1:0] inject_a = 0;
    logic [DATA_W-1:0] inject_b = 0;
    logic root_done;
    logic [DATA_W-1:0] root_result;
    logic [GRID_N*GRID_N-1:0] sleep_mask;

    always #5 clk = ~clk;

    frostbite_grid #(.GRID_N(GRID_N), .DATA_W(DATA_W)) dut (
        .clk         (clk),
        .rst_n       (rst_n),
        .inject_valid(inject_valid),
        .inject_op   (inject_op),
        .inject_a    (inject_a),
        .inject_b    (inject_b),
        .root_done   (root_done),
        .root_result (root_result),
        .sleep_mask  (sleep_mask)
    );

    initial begin
        $dumpfile("frostbite_wave.vcd");
        $dumpvars(0, frostbite_tb);

        #20 rst_n = 1;
        // Вбрасываем тестовую редукцию: (15 * 4) = 60
        @(posedge clk);
        inject_valid <= 1'b1;
        inject_op    <= 3'd2; // MUL
        inject_a     <= 32'd15;
        inject_b     <= 32'd4;

        @(posedge clk);
        inject_valid <= 1'b0;

        wait(root_done);
        @(posedge clk);
        $display("[FROSTBITE RTL] Reduction complete! Result = %0d | Sleep mask = %b", root_result, sleep_mask);
        #20 $finish;
    end

endmodule
"#,
        n = grid_size,
        expr = expr
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


