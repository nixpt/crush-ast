" Crush: // line comments (the lexer also accepts #), 4-space indent
" (matches the polish formatter).
if exists('b:did_ftplugin') | finish | endif
let b:did_ftplugin = 1
setlocal commentstring=//\ %s
setlocal comments=s1:/*,mb:*,ex:*/,://
setlocal expandtab shiftwidth=4 softtabstop=4 tabstop=4
setlocal formatoptions-=t formatoptions+=croql
let b:undo_ftplugin = 'setlocal commentstring< comments< expandtab< shiftwidth< softtabstop< tabstop< formatoptions<'
