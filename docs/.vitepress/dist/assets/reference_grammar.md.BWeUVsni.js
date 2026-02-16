import{_ as s,o as a,c as t,ag as p}from"./chunks/framework.DEqXEGcv.js";const d=JSON.parse('{"title":"Grammar","description":"","frontmatter":{},"headers":[],"relativePath":"reference/grammar.md","filePath":"reference/grammar.md"}'),e={name:"reference/grammar.md"};function o(i,n,l,u,c,q){return a(),t("div",null,[...n[0]||(n[0]=[p(`<h1 id="grammar" tabindex="-1">Grammar <a class="header-anchor" href="#grammar" aria-label="Permalink to &quot;Grammar&quot;">​</a></h1><p>A simplified EBNF grammar for the SMQL language.</p><h2 id="conventions" tabindex="-1">Conventions <a class="header-anchor" href="#conventions" aria-label="Permalink to &quot;Conventions&quot;">​</a></h2><ul><li><code>UPPER_CASE</code> — keywords (case-insensitive)</li><li><code>lower_case</code> — grammar rules</li><li><code>&quot;...&quot;</code> — literal tokens</li><li><code>[ ... ]</code> — optional</li><li><code>{ ... }</code> — zero or more repetitions</li><li><code>( ... | ... )</code> — alternatives</li></ul><h2 id="top-level" tabindex="-1">Top-Level <a class="header-anchor" href="#top-level" aria-label="Permalink to &quot;Top-Level&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>program         = { statement } ;</span></span>
<span class="line"><span>statement       = command | query ;</span></span>
<span class="line"><span>command         = define_machine | spawn | transition | try_transition</span></span>
<span class="line"><span>                | transition_all | alter_machine ;</span></span>
<span class="line"><span>query           = get | find | aggregate | trail | paths | funnel | compare_paths ;</span></span></code></pre></div><h2 id="machine-definition" tabindex="-1">Machine Definition <a class="header-anchor" href="#machine-definition" aria-label="Permalink to &quot;Machine Definition&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>define_machine  = DEFINE MACHINE ident &quot;(&quot; machine_body &quot;)&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>machine_body    = { machine_clause } ;</span></span>
<span class="line"><span>machine_clause  = data_block | states_block | initial_state</span></span>
<span class="line"><span>                | terminal_states | children_block | parent_decl</span></span>
<span class="line"><span>                | transitions_block | hooks_block ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>data_block      = DATA &quot;{&quot; { data_field } &quot;}&quot; ;</span></span>
<span class="line"><span>data_field      = ident &quot;:&quot; type_expr [ &quot;-&gt;&quot; constraints ] ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>type_expr       = TEXT | INT | FLOAT | BOOL | UUID | DATE | DATETIME</span></span>
<span class="line"><span>                | DURATION | MONEY &quot;(&quot; ident &quot;)&quot; | BLOB | JSON</span></span>
<span class="line"><span>                | ENUM &quot;(&quot; ident { &quot;,&quot; ident } &quot;)&quot;</span></span>
<span class="line"><span>                | REF &quot;(&quot; ident &quot;)&quot;</span></span>
<span class="line"><span>                | LIST &quot;(&quot; type_expr &quot;)&quot;</span></span>
<span class="line"><span>                | SET &quot;(&quot; type_expr &quot;)&quot;</span></span>
<span class="line"><span>                | MAP &quot;(&quot; type_expr &quot;,&quot; type_expr &quot;)&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>constraints     = constraint { &quot;,&quot; constraint } ;</span></span>
<span class="line"><span>constraint      = REQUIRED | OPTIONAL</span></span>
<span class="line"><span>                | DEFAULT &quot;(&quot; literal &quot;)&quot;</span></span>
<span class="line"><span>                | MIN &quot;(&quot; number &quot;)&quot;</span></span>
<span class="line"><span>                | MAX &quot;(&quot; number &quot;)&quot;</span></span>
<span class="line"><span>                | RANGE &quot;(&quot; number &quot;,&quot; number &quot;)&quot;</span></span>
<span class="line"><span>                | UNIQUE</span></span>
<span class="line"><span>                | PATTERN &quot;(&quot; string &quot;)&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>states_block    = STATES &quot;{&quot; ident { &quot;,&quot; ident } &quot;}&quot; ;</span></span>
<span class="line"><span>initial_state   = INITIAL STATE ident ;</span></span>
<span class="line"><span>terminal_states = TERMINAL STATES &quot;{&quot; ident { &quot;,&quot; ident } &quot;}&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>children_block  = CHILDREN &quot;{&quot; { child_field } &quot;}&quot; ;</span></span>
<span class="line"><span>child_field     = ident &quot;:&quot; child_type [ &quot;-&gt;&quot; constraints ] ;</span></span>
<span class="line"><span>child_type      = LIST &quot;(&quot; ident &quot;)&quot; | OPTIONAL &quot;(&quot; ident &quot;)&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>parent_decl     = PARENT &quot;:&quot; ident ;</span></span></code></pre></div><h2 id="transitions" tabindex="-1">Transitions <a class="header-anchor" href="#transitions" aria-label="Permalink to &quot;Transitions&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>transitions_block = TRANSITIONS &quot;{&quot; { transition_def } &quot;}&quot; ;</span></span>
<span class="line"><span>transition_def    = transition_source &quot;-&gt;&quot; ident &quot;{&quot; { transition_clause } &quot;}&quot; ;</span></span>
<span class="line"><span>transition_source = ident | ANY ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>transition_clause = guard_clause | action_clause | timeout_clause</span></span>
<span class="line"><span>                  | mutate_clause | except_clause ;</span></span>
<span class="line"><span>guard_clause      = GUARD &quot;:&quot; expression ;</span></span>
<span class="line"><span>action_clause     = ACTION &quot;:&quot; action ;</span></span>
<span class="line"><span>timeout_clause    = TIMEOUT &quot;:&quot; duration &quot;-&gt;&quot; ident ;</span></span>
<span class="line"><span>mutate_clause     = MUTATE &quot;:&quot; ident &quot;=&quot; expression ;</span></span>
<span class="line"><span>except_clause     = EXCEPT FROM &quot;{&quot; ident { &quot;,&quot; ident } &quot;}&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>action            = LOG &quot;(&quot; string &quot;)&quot;</span></span>
<span class="line"><span>                  | NOTIFY &quot;(&quot; expression &quot;,&quot; string &quot;)&quot;</span></span>
<span class="line"><span>                  | EMIT &quot;(&quot; string [ &quot;,&quot; expression ] &quot;)&quot;</span></span>
<span class="line"><span>                  | WEBHOOK &quot;(&quot; string [ &quot;,&quot; expression ] &quot;)&quot; ;</span></span></code></pre></div><h2 id="hooks" tabindex="-1">Hooks <a class="header-anchor" href="#hooks" aria-label="Permalink to &quot;Hooks&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>hooks_block     = HOOKS &quot;{&quot; { hook_def } &quot;}&quot; ;</span></span>
<span class="line"><span>hook_def        = ON SPAWN &quot;{&quot; { action } &quot;}&quot;</span></span>
<span class="line"><span>                | BEFORE EACH TRANSITION &quot;{&quot; { action } &quot;}&quot;</span></span>
<span class="line"><span>                | AFTER EACH TRANSITION &quot;{&quot; { action } &quot;}&quot;</span></span>
<span class="line"><span>                | ON ENTER ident &quot;{&quot; { action } &quot;}&quot;</span></span>
<span class="line"><span>                | ON EXIT ident &quot;{&quot; { action } &quot;}&quot; ;</span></span></code></pre></div><h2 id="commands" tabindex="-1">Commands <a class="header-anchor" href="#commands" aria-label="Permalink to &quot;Commands&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>spawn           = SPAWN ident &quot;{&quot; [ data_pairs ] &quot;}&quot;</span></span>
<span class="line"><span>                  [ THEN TRANSITION TO ident ] ;</span></span>
<span class="line"><span>data_pairs      = data_pair { &quot;,&quot; data_pair } ;</span></span>
<span class="line"><span>data_pair       = ident &quot;:&quot; expression ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>transition      = TRANSITION string TO ident</span></span>
<span class="line"><span>                  [ AS &quot;{&quot; data_pairs &quot;}&quot; ]</span></span>
<span class="line"><span>                  [ WITH &quot;{&quot; data_pairs &quot;}&quot; ]</span></span>
<span class="line"><span>                  [ MEMO string ]</span></span>
<span class="line"><span>                  [ THROUGH ident { &quot;,&quot; ident } ]</span></span>
<span class="line"><span>                  [ OR_STAY ]</span></span>
<span class="line"><span>                  [ CASCADE ] ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>try_transition  = TRY TRANSITION string TO ident</span></span>
<span class="line"><span>                  [ AS &quot;{&quot; data_pairs &quot;}&quot; ]</span></span>
<span class="line"><span>                  [ WITH &quot;{&quot; data_pairs &quot;}&quot; ]</span></span>
<span class="line"><span>                  [ MEMO string ] ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>transition_all  = TRANSITION ALL ident WHERE expression TO ident ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>alter_machine   = ALTER MACHINE ident &quot;(&quot; { alter_op } &quot;)&quot; ;</span></span>
<span class="line"><span>alter_op        = ADD STATE ident</span></span>
<span class="line"><span>                | REMOVE STATE ident</span></span>
<span class="line"><span>                | ADD TRANSITION ident &quot;-&gt;&quot; ident &quot;{&quot; { transition_clause } &quot;}&quot;</span></span>
<span class="line"><span>                | REMOVE TRANSITION ident &quot;-&gt;&quot; ident</span></span>
<span class="line"><span>                | ADD DATA &quot;{&quot; data_field &quot;}&quot;</span></span>
<span class="line"><span>                | REMOVE DATA ident</span></span>
<span class="line"><span>                | BACKFILL &quot;{&quot; data_pairs &quot;}&quot; ;</span></span></code></pre></div><h2 id="queries" tabindex="-1">Queries <a class="header-anchor" href="#queries" aria-label="Permalink to &quot;Queries&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>get             = GET string ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>find            = FIND ident</span></span>
<span class="line"><span>                  [ WHERE expression ]</span></span>
<span class="line"><span>                  [ SORT BY sort_clause { &quot;,&quot; sort_clause } ]</span></span>
<span class="line"><span>                  [ LIMIT number ]</span></span>
<span class="line"><span>                  [ OFFSET number ] ;</span></span>
<span class="line"><span>sort_clause     = ident ( ASC | DESC ) ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>aggregate       = COUNT ident [ WHERE expression ] [ GROUP BY ident ]</span></span>
<span class="line"><span>                | ( SUM | AVG | MIN | MAX ) &quot;(&quot; ident &quot;)&quot; FROM ident</span></span>
<span class="line"><span>                  [ WHERE expression ] [ GROUP BY ident ] ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>trail           = TRAIL OF string ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>paths           = PATHS ident ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>funnel          = FUNNEL ident THROUGH ident { &quot;,&quot; ident } ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>compare_paths   = COMPARE PATHS ident SEGMENT BY ident ;</span></span></code></pre></div><h2 id="expressions" tabindex="-1">Expressions <a class="header-anchor" href="#expressions" aria-label="Permalink to &quot;Expressions&quot;">​</a></h2><div class="language-txt vp-adaptive-theme"><button title="Copy Code" class="copy"></button><span class="lang">txt</span><pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span>expression      = or_expr ;</span></span>
<span class="line"><span>or_expr         = and_expr { OR and_expr } ;</span></span>
<span class="line"><span>and_expr        = not_expr { AND not_expr } ;</span></span>
<span class="line"><span>not_expr        = NOT not_expr | comparison ;</span></span>
<span class="line"><span>comparison      = addition [ comp_op addition ]</span></span>
<span class="line"><span>                | addition IS SET</span></span>
<span class="line"><span>                | addition IS NOT SET</span></span>
<span class="line"><span>                | addition IN &quot;(&quot; expression { &quot;,&quot; expression } &quot;)&quot; ;</span></span>
<span class="line"><span>comp_op         = &quot;==&quot; | &quot;!=&quot; | &quot;&gt;&quot; | &quot;&lt;&quot; | &quot;&gt;=&quot; | &quot;&lt;=&quot; ;</span></span>
<span class="line"><span>addition        = multiplication { ( &quot;+&quot; | &quot;-&quot; ) multiplication } ;</span></span>
<span class="line"><span>multiplication  = unary { ( &quot;*&quot; | &quot;/&quot; | &quot;%&quot; ) unary } ;</span></span>
<span class="line"><span>unary           = [ &quot;-&quot; ] primary ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>primary         = literal | ident [ &quot;.&quot; ident ]</span></span>
<span class="line"><span>                | function_call</span></span>
<span class="line"><span>                | &quot;(&quot; expression &quot;)&quot;</span></span>
<span class="line"><span>                | &quot;{&quot; data_pairs &quot;}&quot;</span></span>
<span class="line"><span>                | ALL &quot;(&quot; ident &quot;,&quot; expression &quot;)&quot;</span></span>
<span class="line"><span>                | ANY &quot;(&quot; ident &quot;,&quot; expression &quot;)&quot;</span></span>
<span class="line"><span>                | STATE IS ident</span></span>
<span class="line"><span>                | STATE IN &quot;(&quot; ident { &quot;,&quot; ident } &quot;)&quot;</span></span>
<span class="line"><span>                | ALIVE | TERMINATED</span></span>
<span class="line"><span>                | SELF | ACTOR [ &quot;.&quot; ident ] ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>function_call   = ident &quot;(&quot; [ expression { &quot;,&quot; expression } ] &quot;)&quot; ;</span></span>
<span class="line"><span></span></span>
<span class="line"><span>literal         = string | number | float | TRUE | FALSE | NULL | duration ;</span></span>
<span class="line"><span>string          = &quot;\\&quot;&quot; { char } &quot;\\&quot;&quot; ;</span></span>
<span class="line"><span>number          = digit { digit } ;</span></span>
<span class="line"><span>float           = number &quot;.&quot; number ;</span></span>
<span class="line"><span>duration        = number ( &quot;s&quot; | &quot;m&quot; | &quot;h&quot; | &quot;d&quot; | &quot;w&quot; ) ;</span></span>
<span class="line"><span>ident           = letter { letter | digit | &quot;_&quot; } ;</span></span></code></pre></div><div class="info custom-block"><p class="custom-block-title">INFO</p><p>This grammar is simplified for readability. The actual parser handles additional edge cases, error recovery, and comment stripping (<code>--</code> line comments).</p></div>`,19)])])}const h=s(e,[["render",o]]);export{d as __pageData,h as default};
