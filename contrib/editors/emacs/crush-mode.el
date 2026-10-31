;;; crush-mode.el --- Major mode for the Crush language  -*- lexical-binding: t; -*-

;; Part of crush-ast (CRUSH-366).  MIT OR Apache-2.0.
;; Package-Requires: ((emacs "27.1"))
;; Keywords: languages

;;; Commentary:

;; `crush-mode' (regexp highlighting, works everywhere) and `crush-ts-mode'
;; (tree-sitter, Emacs >= 29).  Install:
;;
;;   (add-to-list 'load-path "/path/to/crush-ast/contrib/editors/emacs")
;;   (require 'crush-mode)
;;
;; For `crush-ts-mode', build the grammar once:
;;   (add-to-list 'treesit-language-source-alist
;;     '(crush "https://github.com/nixpt/crush-ast" nil "crates/tree-sitter-crush/src"))
;;   M-x treesit-install-language-grammar RET crush RET

;;; Code:

(defgroup crush nil "Crush language support." :group 'languages)

(defcustom crush-indent-offset 4
  "Spaces per indent level (matches the polish formatter)."
  :type 'integer :group 'crush)

(defconst crush-keywords
  '("fn" "let" "struct" "pub" "async" "if" "else" "while" "for" "in" "return"
    "break" "continue" "match" "case" "fallback" "try" "catch" "throw" "spawn"
    "await" "yield" "new" "import" "use" "as" "capability" "not"))

(defconst crush-constants '("true" "false" "null"))

(defvar crush-mode-syntax-table
  (let ((st (make-syntax-table)))
    ;; // and /* */ comments, # as an alias line comment
    (modify-syntax-entry ?/ ". 124b" st)
    (modify-syntax-entry ?* ". 23" st)
    (modify-syntax-entry ?\n "> b" st)
    (modify-syntax-entry ?# "< b" st)
    (modify-syntax-entry ?\" "\"" st)
    (modify-syntax-entry ?' "\"" st)
    (modify-syntax-entry ?\\ "\\" st)
    (modify-syntax-entry ?_ "_" st)
    st))

(defvar crush-font-lock-keywords
  `((,(regexp-opt crush-keywords 'symbols) . font-lock-keyword-face)
    (,(regexp-opt crush-constants 'symbols) . font-lock-constant-face)
    ("\\<fn[ \t]+\\([A-Za-z_][A-Za-z0-9_]*\\)" 1 font-lock-function-name-face)
    ("\\<struct[ \t]+\\([A-Za-z_][A-Za-z0-9_]*\\)" 1 font-lock-type-face)
    ;; annotations and polyglot blocks: @module {, @python {
    ("@\\([A-Za-z_][A-Za-z0-9_-]*\\)" 1 font-lock-preprocessor-face)
    ;; calls, incl. capability calls like fs.read(
    ("\\([A-Za-z_][A-Za-z0-9_.]*\\)(" 1 font-lock-function-call-face)
    ("\\_<-?\\(?:0[xX][0-9a-fA-F]+\\|0[bB][01]+\\|[0-9]+\\(?:\\.[0-9]+\\)?\\)\\_>" . font-lock-constant-face)))

(defun crush--indent-line ()
  "Indent by brace depth, `crush-indent-offset' spaces per level."
  (let ((indent
         (save-excursion
           (back-to-indentation)
           (let ((depth (car (syntax-ppss))))
             (when (looking-at "[]})]") (setq depth (1- depth)))
             (* (max depth 0) crush-indent-offset)))))
    (if (<= (current-column) (current-indentation))
        (indent-line-to indent)
      (save-excursion (indent-line-to indent)))))

;;;###autoload
(define-derived-mode crush-mode prog-mode "Crush"
  "Major mode for Crush source."
  (setq-local font-lock-defaults '(crush-font-lock-keywords))
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-start-skip "\\(?://+\\|#+\\)\\s *")
  (setq-local indent-line-function #'crush--indent-line)
  (setq-local indent-tabs-mode nil))

;;;###autoload
(define-derived-mode crush-asm-mode prog-mode "Crush-ASM"
  "Major mode for CVM1 assembly (.cvms)."
  (setq-local font-lock-defaults
              '((("^\\.func\\>\\s *\\(\\S-+\\)" 1 font-lock-function-name-face)
                 ("^\\.[a-z]+" . font-lock-keyword-face)
                 ("^\\s *\\([A-Z][A-Z0-9_]*\\)\\>" 1 font-lock-builtin-face)
                 ("\\<L[0-9]+:?" . font-lock-constant-face))))
  (setq-local comment-start "// ")
  (setq-local indent-tabs-mode nil))

;;; tree-sitter (Emacs 29+) ---------------------------------------------------

(when (and (fboundp 'treesit-available-p) (treesit-available-p))
  (defvar crush-ts-mode--font-lock-rules
    (treesit-font-lock-rules
     :language 'crush :feature 'comment '((comment) @font-lock-comment-face)
     :language 'crush :feature 'string '((string_literal) @font-lock-string-face)
     :language 'crush :feature 'number
     '([(int_literal) (float_literal)] @font-lock-number-face)
     :language 'crush :feature 'constant
     '([(bool_literal) (null_literal)] @font-lock-constant-face)
     :language 'crush :feature 'keyword
     `([,@(mapcar (lambda (k) k) (seq-difference crush-keywords '("not" "yield")))]
       @font-lock-keyword-face
       (yield_expr) @font-lock-keyword-face)
     :language 'crush :feature 'definition
     '((fn_def name: (identifier) @font-lock-function-name-face)
       (struct_def name: (identifier) @font-lock-type-face)
       (lambda_parameter name: (identifier) @font-lock-variable-name-face))
     :language 'crush :feature 'type '((type) @font-lock-type-face)
     :language 'crush :feature 'function
     '((capability_call name: (identifier) @font-lock-function-call-face)
       (capability_call name: (field_access field: (identifier) @font-lock-function-call-face)))
     :language 'crush :feature 'annotation
     '((annotation name: _ @font-lock-preprocessor-face)
       (lang_block language: (identifier) @font-lock-preprocessor-face))))

  (define-derived-mode crush-ts-mode prog-mode "Crush[ts]"
    "Tree-sitter major mode for Crush."
    (when (treesit-ready-p 'crush)
      (treesit-parser-create 'crush)
      (setq-local treesit-font-lock-settings crush-ts-mode--font-lock-rules)
      (setq-local treesit-font-lock-feature-list
                  '((comment string) (keyword constant number definition type)
                    (function annotation) ()))
      (setq-local treesit-simple-indent-rules
                  `((crush ((node-is "}") parent-bol 0)
                           ((node-is ")") parent-bol 0)
                           ((node-is "]") parent-bol 0)
                           ((parent-is "block") parent-bol ,crush-indent-offset)
                           ((parent-is "annotation_body") parent-bol ,crush-indent-offset)
                           ((parent-is "program") column-0 0))))
      (setq-local comment-start "// ")
      (setq-local indent-tabs-mode nil)
      (treesit-major-mode-setup))))

;;;###autoload
(add-to-list 'auto-mode-alist
             (cons "\\.crush\\'" (if (and (fboundp 'treesit-ready-p)
                                          (ignore-errors (treesit-ready-p 'crush t)))
                                     'crush-ts-mode 'crush-mode)))
;;;###autoload
(add-to-list 'auto-mode-alist '("\\.cvms\\'" . crush-asm-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("\\.\\(?:cast\\.json\\|cast\\|casm\\|sno\\|casm\\.json\\)\\'" . js-json-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/capsule\\.toml\\'" . conf-toml-mode))

(provide 'crush-mode)
;;; crush-mode.el ends here
