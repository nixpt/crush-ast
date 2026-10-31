" Filetype detection for the Crush file formats (CRUSH-366).
autocmd BufRead,BufNewFile *.crush                setfiletype crush
autocmd BufRead,BufNewFile *.cvms                 setfiletype crushasm
" JSON-encoded CAST / CASM; the .cast.json rule must beat the generic *.json one.
" (`set`, not `setfiletype`: Vim already maps *.sno to snobol4.)
autocmd BufRead,BufNewFile *.cast.json,*.cast,*.casm,*.sno,*.casm.json set filetype=json
" crush-pkg manifest
autocmd BufRead,BufNewFile capsule.toml           setfiletype toml
" .castb / .casmb / .cvm1 / .cap / .ecap are binary: leave them to `xxd`/hexedit.
