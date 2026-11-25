use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Fib,
    Fact,
}

impl Op {
    pub fn label(&self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Fib => "Fib",
            Op::Fact => "Fact",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AgentNode {
    pub id: u64,
    pub parent: Option<(u64, usize)>,
    pub op: Op,
    pub args: Vec<Option<i64>>,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileState {
    Sleeping,
    Computing,
    Stealing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// Перехват невычисленной ветки графа (Work-Stealing)
    Steal,
    /// Возврат готового числа родительскому узлу
    Result,
}

#[derive(Clone, Copy, Debug)]
pub struct TileLink {
    pub from_tile: usize,
    pub to_tile: usize,
    pub kind: LinkKind,
}

#[derive(Clone, Debug)]
pub struct Tile {
    pub tasks: Vec<AgentNode>,
    pub inbox: Vec<(u64, usize, i64)>,
    pub state: TileState,
    pub energy_used: f64,
    pub reductions_done: u64,
    pub transfers_done: u64,
}

impl Tile {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            inbox: Vec::new(),
            state: TileState::Sleeping,
            energy_used: 0.0,
            reductions_done: 0,
            transfers_done: 0,
        }
    }
}

pub struct CrystalGrid {
    pub size: usize,
    pub tiles: Vec<Tile>,
    pub tick: u64,
    pub total_reductions: u64,
    pub total_energy: f64,
    pub classical_cpu_energy: f64,
    pub node_locations: HashMap<u64, usize>,
    pub next_id: u64,
    pub final_result: Option<i64>,
    pub last_error: Option<String>,
    /// Активные потоки между плитками на текущем такте (для отрисовки стрелок)
    pub active_links: Vec<TileLink>,
}

impl CrystalGrid {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            tiles: vec![Tile::new(); size * size],
            tick: 0,
            total_reductions: 0,
            total_energy: 0.0,
            classical_cpu_energy: 0.0,
            node_locations: HashMap::new(),
            next_id: 1,
            final_result: None,
            last_error: None,
            active_links: Vec::new(),
        }
    }

    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.size + x
    }

    pub fn coords(&self, idx: usize) -> (usize, usize) {
        (idx % self.size, idx / self.size)
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn submit_expression(&mut self, expr: &str, x: usize, y: usize) {
        self.final_result = None;
        self.last_error = None;
        self.active_links.clear();
        let tokens = match tokenize(expr) {
            Ok(t) => t,
            Err(e) => {
                self.last_error = Some(e);
                return;
            }
        };
        if tokens.is_empty() {
            self.last_error = Some("Пустое выражение".to_string());
            return;
        }

        let mut pos = 0;
        let ast = match parse_expr(&tokens, &mut pos) {
            Ok(a) => a,
            Err(e) => {
                self.last_error = Some(e);
                return;
            }
        };
        if pos < tokens.len() {
            self.last_error = Some(format!("Лишний символ: {:?}", tokens[pos]));
            return;
        }

        let tile_id = self.idx(x.min(self.size - 1), y.min(self.size - 1));
        self.spawn_ast(&ast, None, tile_id);
        self.tiles[tile_id].state = TileState::Computing;
    }

    fn spawn_ast(&mut self, ast: &Ast, parent: Option<(u64, usize)>, tile_id: usize) -> Option<i64> {
        match ast {
            Ast::Num(v) => Some(*v),
            Ast::BinOp(op, lhs, rhs) => {
                let id = self.alloc_id();
                let left_val = self.spawn_ast(lhs, Some((id, 0)), tile_id);
                let right_val = self.spawn_ast(rhs, Some((id, 1)), tile_id);
                let node = AgentNode {
                    id,
                    parent,
                    op: *op,
                    args: vec![left_val, right_val],
                    expanded: true,
                };
                self.node_locations.insert(id, tile_id);
                self.tiles[tile_id].tasks.push(node);
                None
            }
            Ast::Call(op, arg) => {
                let id = self.alloc_id();
                let arg_val = self.spawn_ast(arg, Some((id, 0)), tile_id);
                let node = AgentNode {
                    id,
                    parent,
                    op: *op,
                    args: vec![arg_val],
                    expanded: false,
                };
                self.node_locations.insert(id, tile_id);
                self.tiles[tile_id].tasks.push(node);
                None
            }
        }
    }

    pub fn step(&mut self) {
        self.tick += 1;
        self.active_links.clear();
        let n = self.size;

        // 0. Доставляем входящие пакеты с готовыми значениями в целевые плитки
        let mut routed_packets: Vec<(usize, usize, (u64, usize, i64))> = Vec::new();
        for src_tile in 0..self.tiles.len() {
            for pkt in self.tiles[src_tile].inbox.drain(..) {
                if let Some(&target_tile) = self.node_locations.get(&pkt.0) {
                    routed_packets.push((src_tile, target_tile, pkt));
                }
            }
        }
        for (src_tile, target_tile, (node_id, slot, val)) in routed_packets {
            if src_tile != target_tile {
                self.active_links.push(TileLink {
                    from_tile: src_tile,
                    to_tile: target_tile,
                    kind: LinkKind::Result,
                });
            }
            if let Some(node) = self.tiles[target_tile]
                .tasks
                .iter_mut()
                .find(|t| t.id == node_id)
            {
                if slot < node.args.len() {
                    node.args[slot] = Some(val);
                }
            }
        }

        // 1. Локальные редукции в каждой плитке
        let mut new_nodes_to_spawn: Vec<(usize, AgentNode)> = Vec::new();
        let mut outgoing_results: Vec<(Option<(u64, usize)>, i64, usize)> = Vec::new();

        for tile_id in 0..self.tiles.len() {
            let ready_idx = self.tiles[tile_id].tasks.iter().position(|node| {
                let all_ready = node.args.iter().all(|a| a.is_some());
                let can_expand = !node.expanded && node.args.first().and_then(|a| *a).is_some();
                all_ready || can_expand
            });

            if let Some(idx) = ready_idx {
                let mut node = self.tiles[tile_id].tasks.remove(idx);
                self.tiles[tile_id].state = TileState::Computing;
                self.tiles[tile_id].reductions_done += 1;
                self.total_reductions += 1;

                self.tiles[tile_id].energy_used += 1.0;
                self.total_energy += 1.0;
                self.classical_cpu_energy += 4.59;

                if !node.expanded {
                    let arg = node.args.first().and_then(|a| *a).unwrap_or(0);
                    node.expanded = true;
                    match node.op {
                        Op::Fib => {
                            if arg <= 1 {
                                let res = arg.max(0);
                                self.node_locations.remove(&node.id);
                                outgoing_results.push((node.parent, res, tile_id));
                            } else {
                                node.op = Op::Add;
                                node.args = vec![None, None];
                                let left_id = self.alloc_id();
                                let right_id = self.alloc_id();

                                new_nodes_to_spawn.push((
                                    tile_id,
                                    AgentNode {
                                        id: left_id,
                                        parent: Some((node.id, 0)),
                                        op: Op::Fib,
                                        args: vec![Some(arg - 1)],
                                        expanded: false,
                                    },
                                ));
                                new_nodes_to_spawn.push((
                                    tile_id,
                                    AgentNode {
                                        id: right_id,
                                        parent: Some((node.id, 1)),
                                        op: Op::Fib,
                                        args: vec![Some(arg - 2)],
                                        expanded: false,
                                    },
                                ));
                                self.tiles[tile_id].tasks.push(node);
                            }
                        }
                        Op::Fact => {
                            if arg <= 1 {
                                self.node_locations.remove(&node.id);
                                outgoing_results.push((node.parent, 1, tile_id));
                            } else {
                                node.op = Op::Mul;
                                node.args = vec![Some(arg), None];
                                let child_id = self.alloc_id();
                                new_nodes_to_spawn.push((
                                    tile_id,
                                    AgentNode {
                                        id: child_id,
                                        parent: Some((node.id, 1)),
                                        op: Op::Fact,
                                        args: vec![Some(arg - 1)],
                                        expanded: false,
                                    },
                                ));
                                self.tiles[tile_id].tasks.push(node);
                            }
                        }
                        _ => {}
                    }
                } else {
                    let a = node.args.first().and_then(|v| *v).unwrap_or(0);
                    let b = node.args.get(1).and_then(|v| *v).unwrap_or(0);
                    let res = match node.op {
                        Op::Add => a + b,
                        Op::Sub => a - b,
                        Op::Mul => a * b,
                        Op::Div => {
                            if b == 0 {
                                self.last_error = Some("Деление на ноль в графе!".to_string());
                                0
                            } else {
                                a / b
                            }
                        }
                        _ => 0,
                    };
                    self.node_locations.remove(&node.id);
                    outgoing_results.push((node.parent, res, tile_id));
                }
            } else if !self.tiles[tile_id].tasks.is_empty() {
                self.tiles[tile_id].state = TileState::Sleeping;
                self.tiles[tile_id].energy_used += 0.05;
                self.total_energy += 0.05;
            } else {
                self.tiles[tile_id].state = TileState::Sleeping;
                self.tiles[tile_id].energy_used += 0.01;
                self.total_energy += 0.01;
            }
        }

        for (tile_id, node) in new_nodes_to_spawn {
            self.node_locations.insert(node.id, tile_id);
            self.tiles[tile_id].tasks.push(node);
        }

        for (parent, val, src_tile) in outgoing_results {
            match parent {
                Some((parent_id, slot)) => {
                    self.tiles[src_tile].inbox.push((parent_id, slot, val));
                    self.tiles[src_tile].energy_used += 0.2;
                    self.total_energy += 0.2;
                }
                None => {
                    self.final_result = Some(val);
                }
            }
        }

        // 2. Перехват задач соседями (Work-Stealing)
        let mut transfers: Vec<(usize, usize, AgentNode)> = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let id = self.idx(x, y);
                if self.tiles[id].tasks.is_empty() {
                    let neighbors = [
                        (x.wrapping_sub(1), y),
                        (x + 1, y),
                        (x, y.wrapping_sub(1)),
                        (x, y + 1),
                    ];
                    for (nx, ny) in neighbors {
                        if nx < n && ny < n {
                            let nid = self.idx(nx, ny);
                            if self.tiles[nid].tasks.len() > 1 {
                                if let Some(pos) =
                                    self.tiles[nid].tasks.iter().rposition(|t| !t.expanded)
                                {
                                    let stolen = self.tiles[nid].tasks.remove(pos);
                                    transfers.push((id, nid, stolen));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        for (dst, src, node) in transfers {
            self.node_locations.insert(node.id, dst);
            self.tiles[dst].tasks.push(node);
            self.tiles[dst].state = TileState::Stealing;
            self.tiles[src].transfers_done += 1;
            self.tiles[dst].energy_used += 0.35;
            self.total_energy += 0.35;
            self.active_links.push(TileLink {
                from_tile: src,
                to_tile: dst,
                kind: LinkKind::Steal,
            });
        }
    }
}


#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(i64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

#[derive(Debug, Clone)]
enum Ast {
    Num(i64),
    BinOp(Op, Box<Ast>, Box<Ast>),
    Call(Op, Box<Ast>),
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '+' => { tokens.push(Token::Plus); i += 1; }
            '-' => { tokens.push(Token::Minus); i += 1; }
            '*' => { tokens.push(Token::Star); i += 1; }
            '/' => { tokens.push(Token::Slash); i += 1; }
            '(' => { tokens.push(Token::LParen); i += 1; }
            ')' => { tokens.push(Token::RParen); i += 1; }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
                let s: String = chars[start..i].iter().collect();
                let val = s.parse::<i64>().map_err(|_| format!("Ошибка числа: {s}"))?;
                tokens.push(Token::Num(val));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() { i += 1; }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Ident(s.to_lowercase()));
            }
            other => return Err(format!("Неизвестный символ: '{other}'")),
        }
    }
    Ok(tokens)
}

fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<Ast, String> {
    let mut node = parse_term(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Plus => { *pos += 1; node = Ast::BinOp(Op::Add, Box::new(node), Box::new(parse_term(tokens, pos)?)); }
            Token::Minus => { *pos += 1; node = Ast::BinOp(Op::Sub, Box::new(node), Box::new(parse_term(tokens, pos)?)); }
            _ => break,
        }
    }
    Ok(node)
}

fn parse_term(tokens: &[Token], pos: &mut usize) -> Result<Ast, String> {
    let mut node = parse_factor(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Star => { *pos += 1; node = Ast::BinOp(Op::Mul, Box::new(node), Box::new(parse_factor(tokens, pos)?)); }
            Token::Slash => { *pos += 1; node = Ast::BinOp(Op::Div, Box::new(node), Box::new(parse_factor(tokens, pos)?)); }
            _ => break,
        }
    }
    Ok(node)
}

fn parse_factor(tokens: &[Token], pos: &mut usize) -> Result<Ast, String> {
    if *pos >= tokens.len() { return Err("Неожиданный конец выражения".to_string()); }
    match &tokens[*pos] {
        Token::Num(v) => { let val = *v; *pos += 1; Ok(Ast::Num(val)) }
        Token::Minus => { *pos += 1; Ok(Ast::BinOp(Op::Sub, Box::new(Ast::Num(0)), Box::new(parse_factor(tokens, pos)?))) }
        Token::LParen => {
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen { return Err("Не закрыта скобка ')'".to_string()); }
            *pos += 1;
            Ok(inner)
        }
        Token::Ident(name) => {
            let func_name = name.clone();
            *pos += 1;
            if *pos >= tokens.len() || tokens[*pos] != Token::LParen { return Err(format!("После '{func_name}' ожидалась '('")); }
            *pos += 1;
            let arg = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen { return Err("Не закрыта скобка функции ')'".to_string()); }
            *pos += 1;
            let op = match func_name.as_str() {
                "fib" => Op::Fib,
                "fact" => Op::Fact,
                _ => return Err(format!("Неизвестная функция: '{func_name}'")),
            };
            Ok(Ast::Call(op, Box::new(arg)))
        }
        other => Err(format!("Неожиданный токен: {other:?}")),
    }
}

// ================= КОМПИЛЯТОР В АППАРАТНЫЕ ПАКЕТЫ ДЛЯ RTL =================

#[derive(Clone, Debug)]
pub struct HardwarePacket {
    /// Код операции в RTL: 0 = ADD, 1 = SUB, 2 = MUL, 3 = DIV
    pub op_code: u8,
    pub op_name: &'static str,
    pub arg_a: i64,
    pub arg_b: i64,
    pub expected: i64,
}

/// Разворачивает пользовательское выражение в последовательность аппаратных редукций
pub fn compile_to_packets(expr: &str) -> Result<(Vec<HardwarePacket>, i64), String> {
    let tokens = tokenize(expr)?;
    if tokens.is_empty() {
        return Err("Пустое выражение".to_string());
    }
    let mut pos = 0;
    let ast = parse_expr(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!("Лишний токен: {:?}", tokens[pos]));
    }

    let mut packets = Vec::new();
    let final_val = emit_ast_packets(&ast, &mut packets)?;
    Ok((packets, final_val))
}

fn emit_ast_packets(ast: &Ast, out: &mut Vec<HardwarePacket>) -> Result<i64, String> {
    match ast {
        Ast::Num(v) => Ok(*v),
        Ast::BinOp(op, lhs, rhs) => {
            let a = emit_ast_packets(lhs, out)?;
            let b = emit_ast_packets(rhs, out)?;
            let (code, name, res) = match op {
                Op::Add => (0, "ADD", a.wrapping_add(b)),
                Op::Sub => (1, "SUB", a.wrapping_sub(b)),
                Op::Mul => (2, "MUL", a.wrapping_mul(b)),
                Op::Div => {
                    if b == 0 {
                        return Err("Деление на ноль при компиляции в RTL".to_string());
                    }
                    (3, "DIV", a / b)
                }
                _ => (0, "ADD", a + b),
            };
            out.push(HardwarePacket {
                op_code: code,
                op_name: name,
                arg_a: a,
                arg_b: b,
                expected: res,
            });
            Ok(res)
        }
        Ast::Call(op, arg_ast) => {
            let n = emit_ast_packets(arg_ast, out)?;
            match op {
                Op::Fib => emit_fib_packets(n, out),
                Op::Fact => emit_fact_packets(n, out),
                _ => Ok(0),
            }
        }
    }
}

fn emit_fib_packets(n: i64, out: &mut Vec<HardwarePacket>) -> Result<i64, String> {
    if n <= 1 {
        return Ok(n.max(0));
    }
    let a = emit_fib_packets(n - 1, out)?;
    let b = emit_fib_packets(n - 2, out)?;
    let res = a.wrapping_add(b);
    out.push(HardwarePacket {
        op_code: 0,
        op_name: "ADD",
        arg_a: a,
        arg_b: b,
        expected: res,
    });
    Ok(res)
}

fn emit_fact_packets(n: i64, out: &mut Vec<HardwarePacket>) -> Result<i64, String> {
    if n <= 1 {
        return Ok(1);
    }
    let sub = emit_fact_packets(n - 1, out)?;
    let res = n.wrapping_mul(sub);
    out.push(HardwarePacket {
        op_code: 2,
        op_name: "MUL",
        arg_a: n,
        arg_b: sub,
        expected: res,
    });
    Ok(res)
}

