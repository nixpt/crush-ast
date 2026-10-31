" CVM1 assembly (.cvms): `.func name` directives, `label:` lines, 4-space
" instructions, `;` / `#` comments (assembler.rs `strip_comment`).
if exists('b:did_ftplugin') | finish | endif
let b:did_ftplugin = 1
setlocal commentstring=;\ %s
setlocal expandtab shiftwidth=4 softtabstop=4
let b:undo_ftplugin = 'setlocal commentstring< expandtab< shiftwidth< softtabstop<'
