// 反重力 Hub 页面里的汉化 / 自动审批 / 高危拦截引擎。
//
// 原作：EasyAntigravity（https://github.com/DSDS-CMHL/EasyAntigravity）
// Copyright (c) 2026 Astwarp — MIT License（全文见同目录 LICENSE.EasyAntigravity）。
// 取自其 server.js 的 generateMasterInjectScript()，逐字保留，只把六处运行时插值换成
// 双下划线包着的 EA_ 占位符，由 QB Gate 的 Rust 侧（plugins::antigravity_ui）在注入前替换：
//   EA_PREFER_OPTION（1–4）、EA_BLOCK_DANGEROUS、EA_AUTO_ACCEPT、EA_ENABLE_I18N（布尔）、
//   EA_DICT（字典 JSON 对象）、EA_PATTERNS（规则 JSON 数组）。
// （这段注释故意不写完整的占位符名：架构测试数的是它们在文件里各恰好出现一次。）
//
// 它通过 Chrome DevTools 协议的 Runtime.evaluate 跑在 Hub 自己的页面里：不改 Hub 的
// 任何文件、不碰它的进程内存、不碰凭据。页面刷新它就没了，`__ea_engine_running` 让重复
// 注入幂等。日志走 console（[EA_AA] 放行 / [EA_ALERT] 拦截 / [EA_OPT] 选项），
// 由 Rust 侧从 Runtime.consoleAPICalled 收回来。
//
// ⛔ 改这份文件要同步改 ATTRIBUTION.md 里「EasyAntigravity」那一节；
// 架构测试 the_injected_engine_keeps_its_placeholders 钉着六个占位符都在。
(() => {
    window.__ea_config = Object.assign(window.__ea_config || {}, {
      preferOption: __EA_PREFER_OPTION__,
      blockDangerous: __EA_BLOCK_DANGEROUS__,
      autoAccept: __EA_AUTO_ACCEPT__,
      enableI18n: __EA_ENABLE_I18N__
    });
    window.__ea_dict = __EA_DICT__;
    window.__ea_danger_patterns = __EA_PATTERNS__;

    if (window.__ea_engine_running) return;
    window.__ea_engine_running = true;

    const DANGEROUS_PATTERNS = (window.__ea_danger_patterns || []).map(r => {
      try { return { id: r.id, name: r.name, re: new RegExp(r.pattern, r.flags || 'i') }; }
      catch (e) { return null; }
    }).filter(Boolean);

    function realClick(el) {
      const opts = { bubbles: true, cancelable: true, view: window };
      el.dispatchEvent(new PointerEvent('pointerdown', opts));
      el.dispatchEvent(new MouseEvent('mousedown', opts));
      el.dispatchEvent(new PointerEvent('pointerup', opts));
      el.dispatchEvent(new MouseEvent('mouseup', opts));
      el.dispatchEvent(new MouseEvent('click', opts));
      el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true }));
      el.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true }));
    }

    function norm(s) {
      return String(s || '').toLowerCase().replace(/\s+/g, ' ').trim();
    }

    function isSubmitLabel(s) {
      const t = norm(s);
      if (!t || t.length > 40) return false;
      return (
        t === 'submit' ||
        t.startsWith('submit') ||
        t === '提交' ||
        t.includes('提交') ||
        t === 'confirm' ||
        t.startsWith('confirm') ||
        t === '确认' ||
        t.startsWith('确认') ||
        t === 'allow' ||
        t === '允许' ||
        t.includes('submit ↵') ||
        t.includes('提交 ↵') ||
        t.includes('submit enter')
      );
    }

    function findSubmitBtn(root) {
      if (!root || !root.querySelector) return null;
      const byTest = root.querySelector(
        'button[data-testid="interaction-continue-button"], [data-testid="interaction-continue-button"]'
      );
      if (byTest && !byTest.disabled && !byTest.hasAttribute('data-ea-ok')) return byTest;
      const btns = Array.from(root.querySelectorAll('button, [role="button"], div[role="button"], input[type="submit"]'));
      return btns.find(b => {
        if (!b || b.disabled || b.hasAttribute('data-ea-ok')) return false;
        return isSubmitLabel(b.innerText || b.value || b.getAttribute('aria-label'));
      }) || null;
    }

    function cardForSubmit(btn) {
      return (
        btn.closest('[data-testid="run-command-step"]') ||
        btn.closest('div.relative.flex.flex-col') ||
        btn.closest('div[class*="card"]') ||
        btn.closest('div[class*="container"]') ||
        btn.parentElement?.parentElement?.parentElement?.parentElement ||
        btn.parentElement
      );
    }

    function oneLine(s, n) {
      s = String(s || '').replace(/\s+/g, ' ').trim();
      if (n && s.length > n) s = s.slice(0, Math.max(1, n - 1)) + '…';
      return s;
    }

    // 高危扫描用：尽量拿到完整命令正文（可多行）
    function extractCommandText(card) {
      if (!card) return '';
      const code = card.querySelector('pre, code, [data-testid="run-command-step"] pre');
      if (code && (code.innerText || code.textContent || '').trim()) {
        return (code.innerText || code.textContent || '').trim();
      }
      const step = card.querySelector('[data-testid="run-command-step"]');
      if (step) return (step.innerText || '').trim();
      return '';
    }

    // 日志用：单行短摘要
    function extractLogSummary(card, fallback) {
      const full = extractCommandText(card);
      if (full) {
        const lines = full.split('\n').map(s => s.trim()).filter(Boolean);
        // 优先问句行，其次命令行
        const q = lines.find(l => /[?？]$/.test(l) && l.length < 120);
        const cmdLine = lines.find(l => !/[?？]$/.test(l) && l.length > 2);
        return oneLine(q || cmdLine || lines[0], 80);
      }
      return oneLine(fallback || '', 80);
    }

    function classifyOption(tx) {
      const t = norm(tx);
      if (!t || t.length > 200) return 0;
      const isAlways = /always allow|始终允许|总是允许|一直允许/.test(t);
      const isThisTime = /\bthis time\b|仅允许本次|仅这一次|只允许本次|仅本次/.test(t);
      const isSession = /\bin this conversation\b|\bthis session\b|\bthis conversation\b|对话中|本次会话|本次对话/.test(t);
      const isProject = /\bin (this|every) project\b|\bthis project\b|项目中|本项目|所有项目/.test(t);
      // 编号优先（1. / 2. / 3. / 4.）
      const num = t.match(/^([1-4])[\s\.\:：\-]/);
      if (num) return Number(num[1]);
      // 互斥语义：this time ≠ always
      if (isThisTime && !isAlways) return 1;
      if (isAlways && isSession && !isProject) return 2;
      if (isAlways && isProject) return 3;
      if (isAlways && !isSession && !isProject && !isThisTime) return 4;
      // 裸编号
      if (t === '1' || t === '2' || t === '3' || t === '4') return Number(t);
      return 0;
    }

    function collectOptionCands(card) {
      const nodes = Array.from(card.querySelectorAll(
        'label, [role="radio"], [role="option"], [data-testid*="option"], [data-testid*="radio"], button, div, span, li'
      ));
      const cands = [];
      const seen = new Set();
      for (const el of nodes) {
        if (!el || seen.has(el)) continue;
        const raw = (el.innerText || el.textContent || '').trim();
        if (!raw || raw.length > 160) continue;
        // 多行选项容器不是叶子选项
        if (raw.includes('\n') && raw.split('\n').filter(Boolean).length > 2) continue;
        // 只要叶子/近叶子：有更深子节点且文本相同则跳过父级，避免点到整块容器
        const kids = el.children ? Array.from(el.children) : [];
        if (kids.length) {
          const kidTexts = kids.map(k => (k.innerText || '').trim()).join(' ');
          if (kidTexts && norm(kidTexts) === norm(raw) && raw.length > 20) continue;
        }
        const cls = classifyOption(raw);
        if (!cls) continue;
        const st = el.getAttribute && el.getAttribute('data-state');
        const checked = (
          el.checked === true ||
          el.getAttribute('aria-checked') === 'true' ||
          st === 'checked' ||
          st === 'on' ||
          (el.classList && el.classList.contains('checked'))
        );
        const score =
          (el.getAttribute && el.getAttribute('role') === 'radio' ? 40 : 0) +
          (el.tagName === 'LABEL' ? 30 : 0) +
          (el.getAttribute && /option|radio|choice/i.test(el.getAttribute('data-testid') || '') ? 35 : 0) +
          (checked ? 10 : 0) -
          Math.min(raw.length, 80) * 0.1;
        cands.push({ el, cls, score, checked, text: clip(raw, 80) });
        seen.add(el);
      }
      return cands;
    }

    function matchOptionEl(card, optIdx) {
      const idx = Number(optIdx) || 4;
      const cands = collectOptionCands(card);
      const exact = cands.filter(c => c.cls === idx);
      if (exact.length) {
        exact.sort((a, b) => b.score - a.score);
        return exact[0];
      }
      // 目标选项不存在时：若只有 this-time 语义选项，优先选它，避免默认落到 always
      if (idx === 1) {
        const t1 = cands.filter(c => c.cls === 1);
        if (t1.length) return t1[0];
      }
      return null;
    }

    function tryApprove(btn, kind) {
      if (!btn || btn.disabled || btn.hasAttribute('data-ea-ok')) return false;
      const card = cardForSubmit(btn);
      if (window.__ea_config.blockDangerous) {
        const cmd = extractCommandText(card);
        if (cmd) {
          const hit = DANGEROUS_PATTERNS.find(r => r.re.test(cmd));
          if (hit) {
            btn.setAttribute('data-ea-ok', 'blocked');
            console.warn('[EA_ALERT] 拦截高危指令[' + hit.id + ']: ' + cmd.slice(0, 80));
            return true;
          }
        }
      }
      const optIdx = (window.__ea_config && window.__ea_config.preferOption) || 4;
      let optText = '';
      let picked = null;
      if (card) {
        const cands = collectOptionCands(card);
        if (cands.length) {
          const brief = cands.map(c => '#' + c.cls + (c.checked ? '*' : '') + oneLine(c.text, 36)).join(' | ');
          console.log('[EA_OPT] prefer=' + optIdx + ' · ' + oneLine(brief, 180));
        }
        picked = matchOptionEl(card, optIdx);
        if (picked && picked.el) {
          realClick(picked.el);
          optText = oneLine(picked.text, 40);
          // 若点击后仍无选中态，再点一次（部分自定义控件首次 pointer 无效）
          const st = picked.el.getAttribute && picked.el.getAttribute('data-state');
          const stillOff = picked.el.checked === false ||
            picked.el.getAttribute('aria-checked') === 'false' ||
            st === 'unchecked';
          if (stillOff) realClick(picked.el);
        } else if (cands.length) {
          // 有选项组但没匹配到目标：不要默默点提交，避免落到会话级 always
          console.warn('[EA_OPT] 未找到选项[' + optIdx + ']，暂不点击提交');
          return false;
        }
      }
      btn.setAttribute('data-ea-ok', 'true');
      realClick(btn);
      const cmd = extractLogSummary(card, kind || (btn.innerText || ''));
      const optLabel = picked
        ? '选项[' + picked.cls + '] ' + optText
        : '无选项组';
      console.log('[EA_AA] 放行 · ' + optLabel + ' · ' + cmd);
      return true;
    }

    function clip(s, n) {
      s = String(s || '').replace(/\s+/g, ' ').trim();
      if (s.length <= n) return s;
      return s.slice(0, n - 1) + '…';
    }

    function extractRequestSummary(card) {
      if (!card) return '';
      const parts = [];
      // 代码/命令块优先
      const codes = Array.from(card.querySelectorAll('pre, code, [class*="command"], [class*="code"], [data-testid*="command"]'));
      for (const c of codes) {
        const t = (c.innerText || c.textContent || '').trim();
        if (t && t.length > 1 && t.length < 500) {
          parts.push(t);
          if (parts.length >= 2) break;
        }
      }
      // 文件路径类
      const paths = Array.from(card.querySelectorAll('[class*="path"], [class*="file"], [title]'));
      for (const p of paths) {
        const t = (p.getAttribute('title') || p.innerText || '').trim();
        if (t && /[\\/]|:\\/.test(t) && t.length < 200) {
          parts.push(t);
          break;
        }
      }
      // 描述段落：排除按钮/选项行
      if (parts.length === 0) {
        const texts = Array.from(card.querySelectorAll('p, span, div'))
          .map(el => (el.innerText || '').trim())
          .filter(t => t.length > 8 && t.length < 180)
          .filter(t => !isSubmitLabel(t))
          .filter(t => !/^(1|2|3|4)[\s\.\:：\-]/.test(norm(t)))
          .filter(t => !/^(submit|提交|confirm|确认|allow|允许|run|运行)/i.test(t));
        if (texts.length) {
          // 取最长的一段作为请求描述
          texts.sort((a, b) => b.length - a.length);
          parts.push(texts[0]);
        }
      }
      const uniq = [];
      for (const p of parts) {
        const v = clip(p, 160);
        if (v && uniq.indexOf(v) < 0) uniq.push(v);
      }
      return uniq.join(' | ');
    }

    function translateDOM(root) {
      if (!window.__ea_config.enableI18n || !window.__ea_dict) return;
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      let node;
      while ((node = walker.nextNode())) {
        const text = node.nodeValue.trim();
        if (text && window.__ea_dict[text]) {
          node.nodeValue = node.nodeValue.replace(text, window.__ea_dict[text]);
        }
      }
      const elements = root.querySelectorAll ? root.querySelectorAll('[placeholder], [title]') : [];
      elements.forEach(el => {
        const ph = el.getAttribute('placeholder');
        if (ph && window.__ea_dict[ph]) el.setAttribute('placeholder', window.__ea_dict[ph]);
        const title = el.getAttribute('title');
        if (title && window.__ea_dict[title]) el.setAttribute('title', window.__ea_dict[title]);
      });
    }

    setInterval(() => {
      function scan(doc) {
        translateDOM(doc);
        if (!window.__ea_config.autoAccept) return;

        // 1) AG 2.13 交互卡（运行命令 / 工具审批）：优先 data-testid
        const interactSubmit = doc.querySelector('button[data-testid="interaction-continue-button"]');
        if (interactSubmit && tryApprove(interactSubmit, '交互卡提交')) return;

        // 2) 权限卡片（收窄匹配，避免正文里的 permission/read files 误伤）
        const pageText = norm(doc.body ? doc.body.innerText : '');
        const permHit = /allow reading|yes, allow|允许访问|allow access|权限请求|请求权限|访问文件/.test(pageText)
          || (/(^|\s)permission(\s|$)/i.test(pageText) && /allow|允许|yes/i.test(pageText));
        if (permHit) {
          const cards = Array.from(doc.querySelectorAll('div, section, [role="dialog"], [role="alertdialog"]'));
          for (const card of cards) {
            const t = norm(card.innerText);
            if (!t || t.length > 2500) continue;
            if (!/allow reading|yes, allow|允许访问|allow access|permission|权限|访问文件/.test(t)) continue;
            const submitBtn = findSubmitBtn(card);
            if (submitBtn && tryApprove(submitBtn, '权限卡')) return;
          }
        }

        // 3) 关键字兜底
        const kws = ['run', 'accept', 'continue', 'always allow', 'allow', '运行', '接受', '继续', '始终允许', '允许', '确认', '提交'];
        for (const btn of Array.from(doc.querySelectorAll('button, [role="button"]'))) {
          const txt = norm(btn.innerText || btn.getAttribute('aria-label'));
          if (!txt || txt.length > 24) continue;
          if (kws.some(k => txt === k || txt.startsWith(k))) {
            if (tryApprove(btn, txt)) return;
          }
        }
      }

      scan(document);
      const iframes = document.querySelectorAll('iframe');
      iframes.forEach(f => {
        try { if (f.contentDocument) scan(f.contentDocument); } catch (e) {}
      });
    }, 800);
  })();