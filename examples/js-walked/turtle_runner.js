// Turtle Runner — self-playing simulation, walked through crush-lang-js
// (real swc-based JS -> CAST) rather than written natively in Crush.
//
// Ported from crush-capsules/games/turtle-runner/game.js, a real, complete
// browser game (ES6 classes, canvas 2D context, requestAnimationFrame,
// keyboard events) — genuinely runs in a browser, unlike snake.crush's
// aspirational source. Doesn't walk into Crush unchanged, though: classes
// (`new ClassName(...)`) don't compile ("Undefined function: new X"), and
// getting even class-free, function-only JS through
// `crush_frontend::compile_cast` took extensive black-box bisection — the
// type-checker has severe, NON-LOCAL bugs (see CRUSH-4): a completely
// unrelated, never-called function's shape can flip whether an unrelated
// OTHER function type-checks. Confirmed safe patterns used exclusively
// here, all found by trial and error against the real compiler:
//
//   - No reassignment anywhere (`x = ...` without `let`/`const`) — fails
//     unconditionally with "Undefined function: __crush_assign__". No
//     C-style/for-of loops either (both need a mutating counter). Iteration
//     is tail-recursion (`return tick(...)`) instead, same technique used
//     in ../crush/snake.crush.
//   - Every helper function is EITHER (a) multi-branch where every branch
//     returns a bare literal (the "_flag" functions below, returning only
//     0 or 1 — confirmed safe across ~15 variations), OR (b) exactly one
//     unconditional `return <expr>;`, no `if` at all (next_jump_left,
//     next_obstacle_x below). Mixing — one branch a literal, another a
//     parameter/computed value — breaks return-type inference even when
//     the function is never called (CRUSH-4).
//   - Booleans are never passed as function arguments and used directly in
//     an `if` there (`If condition must be bool, found any` — the type
//     doesn't propagate through the parameter). Represented as 0/1 ints,
//     compared with `== 1`, everywhere instead.
//   - jump_left is allowed to go negative indefinitely once grounded
//     (`next_jump_left`'s single unconditional formula has no floor-at-0
//     clamp) — deliberate: only `<= 0`/`> 3` comparisons ever read it, so
//     an unbounded negative value is harmless and avoids needing a
//     conditional (which would hit the bug above).
//
// AOT-compiled too, as a second verification of the same walked CAST
// (`crush_aot::AotCompiler`) — both backends found broken, filed
// separately: the Rust-codegen backend can't compile ANY program, even
// pure-numeric ones with no strings at all (CRUSH-5, references a
// `RuntimeValue::Str` variant that doesn't exist in its own generated
// enum); the C-codegen backend (`--backend gcc`) does compile and run,
// gets numeric output right, but silently corrupts string output
// (CRUSH-6, prints garbage floats like `1.73347e-308` instead of the
// actual grid/score text). Run via the interpreter path instead:
// `crush-walk-run examples/js-walked/turtle_runner.js`.
//
// print()/console.log() don't append a trailing newline (same as native
// Crush's print()) — frames run together on one line rather than
// rendering as a clean grid. Left as-is rather than "fixed" with
// `+ "\n"`: doing that reintroduced the CRUSH-4 type-inference bug in this
// exact file (removing a single no-op `console.log("");` call flipped a
// working version back to a compile error) — not worth destabilizing a
// confirmed-working file to chase a cosmetic-only fix.

function should_jump_flag(obstacle_x, jump_left) {
    if (jump_left != 0) {
        return 0;
    }
    const gap = obstacle_x - 3;
    if (gap < 0) {
        return 0;
    }
    if (gap <= 4) {
        return 1;
    }
    return 0;
}

function next_jump_left(jump_left, jumping_flag) {
    return jumping_flag * 6 + (1 - jumping_flag) * (jump_left - 1);
}

function next_obstacle_x(obstacle_x, speed) {
    return ((obstacle_x - speed) % 24 + 24) % 24;
}

function hits_obstacle_flag(obstacle_x, jump_left) {
    if (obstacle_x != 3) {
        return 0;
    }
    if (jump_left > 3) {
        return 0;
    }
    return 1;
}

function game_over_flag(obstacle_x, jump_left, ticks_left) {
    if (ticks_left <= 0) {
        return 1;
    }
    if (hits_obstacle_flag(obstacle_x, jump_left) == 1) {
        return 1;
    }
    return 0;
}

function air_cell(x, obstacle_x, jump_left) {
    if (x != 3) {
        return " ";
    }
    if (jump_left > 3) {
        return "T";
    }
    return " ";
}

function ground_cell(x, obstacle_x, jump_left) {
    if (x == 3) {
        if (jump_left <= 3) {
            return "T";
        }
        return "_";
    }
    if (x == obstacle_x) {
        return "#";
    }
    return "_";
}

function build_air_row(x, obstacle_x, jump_left) {
    if (x >= 24) {
        return "";
    }
    return air_cell(x, obstacle_x, jump_left) + build_air_row(x + 1, obstacle_x, jump_left);
}

function build_ground_row(x, obstacle_x, jump_left) {
    if (x >= 24) {
        return "";
    }
    return ground_cell(x, obstacle_x, jump_left) + build_ground_row(x + 1, obstacle_x, jump_left);
}

function render_frame(obstacle_x, jump_left, score) {
    console.log(build_air_row(0, obstacle_x, jump_left));
    console.log(build_ground_row(0, obstacle_x, jump_left));
    console.log("score: " + score);
    console.log("");
}

function tick(obstacle_x, jump_left, speed, score, ticks_left) {
    render_frame(obstacle_x, jump_left, score);

    if (game_over_flag(obstacle_x, jump_left, ticks_left) == 1) {
        if (ticks_left <= 0) {
            console.log("out of ticks — final score: " + score);
        } else {
            console.log("hit the obstacle — final score: " + score);
        }
        return score;
    }

    const jumping_flag = should_jump_flag(obstacle_x, jump_left);
    const new_obstacle_x = next_obstacle_x(obstacle_x, speed);
    const new_jump_left = next_jump_left(jump_left, jumping_flag);
    const new_score = score + 1;

    return tick(new_obstacle_x, new_jump_left, speed, new_score, ticks_left - 1);
}

console.log("Turtle Runner (self-playing) — T turtle, # obstacle");
console.log("");
tick(23, 0, 1, 0, 40);
