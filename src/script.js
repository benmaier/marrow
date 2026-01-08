// Initialize from settings injected by Rust
let currentMode = initialSettings.view_mode || 'github';
let tocVisible = initialSettings.toc_visible || false;
let fontSizeLevel = initialSettings.font_size_level || 0;
const TOC_WIDTH = 200;
const BASE_FONT_SIZE = 15;
const TERMINAL_BASE_SIZE = 11;

function saveSettings() {
    if (window.ipc) {
        // Get current window size (subtract TOC width if visible)
        const width = window.innerWidth - (tocVisible ? TOC_WIDTH : 0);
        const height = window.innerHeight;
        const settings = {
            window_width: width,
            window_height: height,
            toc_visible: tocVisible,
            view_mode: currentMode,
            font_size_level: fontSizeLevel
        };
        window.ipc.postMessage('save_settings:' + JSON.stringify(settings));
    }
}

function adjustFontSize(delta) {
    fontSizeLevel = Math.max(-3, Math.min(5, fontSizeLevel + delta));
    applyFontSize();
    saveSettings();
}

function resetFontSize() {
    fontSizeLevel = 0;
    applyFontSize();
    saveSettings();
}

function applyFontSize() {
    const scale = 1 + (fontSizeLevel * 0.1);  // Each level is 10%
    document.body.style.fontSize = (BASE_FONT_SIZE * scale) + 'px';
    document.getElementById('terminal-view').style.fontSize = (TERMINAL_BASE_SIZE * scale) + 'px';
}

function getCurrentHeadingId() {
    const content = document.getElementById('content');
    const activeView = currentMode === 'github' ? '#github-view' : '#terminal-view';
    const headings = document.querySelectorAll(activeView + ' h1, ' + activeView + ' h2, ' + activeView + ' h3, ' + activeView + ' h4, ' + activeView + ' h5, ' + activeView + ' h6, ' + activeView + ' [id].md-heading');

    let currentHeading = null;
    const scrollTop = content.scrollTop;

    for (const heading of headings) {
        if (heading.offsetTop <= scrollTop + 100) {
            currentHeading = heading;
        } else {
            break;
        }
    }

    return currentHeading ? currentHeading.id : null;
}

function setMode(mode, scrollToId) {
    currentMode = mode;
    const content = document.getElementById('content');
    content.className = 'content ' + mode;

    // Toggle views
    document.getElementById('github-view').style.display = mode === 'github' ? 'block' : 'none';
    document.getElementById('terminal-view').style.display = mode === 'terminal' ? 'block' : 'none';

    // Scroll to the same section in the new view
    if (scrollToId) {
        const newView = mode === 'github' ? '#github-view' : '#terminal-view';
        const el = document.querySelector(newView + ' #' + CSS.escape(scrollToId));
        if (el) {
            el.scrollIntoView({ block: 'start' });
        }
    }

    saveSettings();
}

function toggleToc() {
    const currentWidth = window.innerWidth;
    const currentHeight = window.innerHeight;

    tocVisible = !tocVisible;
    const toc = document.getElementById('toc');
    toc.classList.toggle('hidden', !tocVisible);

    // Resize window via IPC - send the new absolute size
    if (window.ipc) {
        let newWidth;
        if (tocVisible) {
            newWidth = currentWidth + TOC_WIDTH;
        } else {
            newWidth = currentWidth - TOC_WIDTH;
        }
        window.ipc.postMessage('resize:' + newWidth + ':' + currentHeight);
    }

    saveSettings();
}

function scrollToHeading(slug) {
    // Find element in the currently active view
    const activeView = currentMode === 'github' ? '#github-view' : '#terminal-view';
    const el = document.querySelector(activeView + ' #' + CSS.escape(slug));
    if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
}

// Extract markdown for selection (used by Cmd+C)
function extractMarkdownForSelection() {
    const selection = window.getSelection();
    if (!selection.rangeCount || selection.isCollapsed) return null;

    try {
        const range = selection.getRangeAt(0);
        let minLine = Infinity;
        let maxLine = 0;

        // Walk up from selection endpoints to find data-lines elements
        let node = range.startContainer;
        while (node && node !== document.body) {
            if (node.nodeType === 1 && node.getAttribute) {
                const dl = node.getAttribute('data-lines');
                if (dl) {
                    const [start, end] = dl.split('-').map(Number);
                    if (start && start < minLine) minLine = start;
                    if (end && end > maxLine) maxLine = end;
                }
            }
            node = node.parentNode;
        }

        node = range.endContainer;
        while (node && node !== document.body) {
            if (node.nodeType === 1 && node.getAttribute) {
                const dl = node.getAttribute('data-lines');
                if (dl) {
                    const [start, end] = dl.split('-').map(Number);
                    if (start && start < minLine) minLine = start;
                    if (end && end > maxLine) maxLine = end;
                }
            }
            node = node.parentNode;
        }

        // Also check elements within the selection range (for block selections)
        const container = range.commonAncestorContainer;
        if (container.nodeType === 1 || container.parentElement) {
            const root = container.nodeType === 1 ? container : container.parentElement;
            const elementsWithDataLines = root.querySelectorAll('[data-lines]');
            elementsWithDataLines.forEach(el => {
                if (selection.containsNode(el, true)) {
                    const dl = el.getAttribute('data-lines');
                    const [start, end] = dl.split('-').map(Number);
                    if (start && start < minLine) minLine = start;
                    if (end && end > maxLine) maxLine = end;
                }
            });
        }

        if (minLine !== Infinity && maxLine > 0 && typeof markdownLines !== 'undefined' && markdownLines.length > 0) {
            const markdownBlock = markdownLines.slice(minLine - 1, maxLine).join('\n');
            const selectedText = selection.toString();
            let extracted = markdownBlock;

            if (selectedText.trim()) {
                const words = selectedText.trim().split(/\s+/);
                const firstWords = words.slice(0, 3).join(' ');
                const lastWords = words.slice(-3).join(' ');

                let startIndex = markdownBlock.indexOf(firstWords);
                if (startIndex === -1 && words.length > 0) {
                    startIndex = markdownBlock.indexOf(words[0]);
                }

                if (startIndex !== -1) {
                    let endIndex = markdownBlock.lastIndexOf(lastWords);
                    if (endIndex === -1 && words.length > 0) {
                        endIndex = markdownBlock.lastIndexOf(words[words.length - 1]);
                    }
                    if (endIndex !== -1) {
                        endIndex += (endIndex === markdownBlock.lastIndexOf(lastWords) ? lastWords.length : words[words.length - 1].length);
                    } else {
                        endIndex = markdownBlock.length;
                    }
                    const syntaxChars = /[*_`#|\[\]()>~-]/;
                    let expandedStart = startIndex;
                    while (expandedStart > 0 && syntaxChars.test(markdownBlock[expandedStart - 1])) expandedStart--;
                    while (expandedStart > 0 && markdownBlock[expandedStart - 1] === ' ') expandedStart--;
                    while (expandedStart > 0 && syntaxChars.test(markdownBlock[expandedStart - 1])) expandedStart--;

                    // If no text (letters) between line start and selection, include full line prefix
                    let lineStart = expandedStart;
                    while (lineStart > 0 && markdownBlock[lineStart - 1] !== '\n') lineStart--;
                    const prefixBeforeSelection = markdownBlock.substring(lineStart, expandedStart);
                    if (!/[a-zA-Z]/.test(prefixBeforeSelection)) {
                        expandedStart = lineStart;
                    }

                    let expandedEnd = endIndex;
                    while (expandedEnd < markdownBlock.length && syntaxChars.test(markdownBlock[expandedEnd])) expandedEnd++;
                    while (expandedEnd < markdownBlock.length && markdownBlock[expandedEnd] === ' ') expandedEnd++;
                    while (expandedEnd < markdownBlock.length && syntaxChars.test(markdownBlock[expandedEnd])) expandedEnd++;

                    // Check for closing code fence - only include if selection reached end of code content
                    const remaining = markdownBlock.substring(expandedEnd);
                    // Only add fence if remaining is ONLY whitespace + fence (nothing else between)
                    const fenceOnlyMatch = remaining.match(/^[\s\n]*```\s*$/);
                    if (fenceOnlyMatch) {
                        expandedEnd = markdownBlock.length;
                    }

                    const candidate = markdownBlock.substring(expandedStart, expandedEnd);
                    if (candidate.length >= selectedText.length * 0.5) {
                        extracted = candidate;
                    }
                }
            }
            return extracted;
        }
    } catch (err) {}
    return null;
}

// Keyboard shortcuts
document.addEventListener('keydown', function(e) {
    // Cmd+F for search
    if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        e.preventDefault();
        openSearch();
        return;
    }

    // Cmd+W to close window
    if (e.metaKey && e.key === 'w') {
        e.preventDefault();
        if (window.ipc) window.ipc.postMessage('close_window');
        return;
    }

    // Cmd+Q to quit app
    if (e.metaKey && e.key === 'q') {
        e.preventDefault();
        if (window.ipc) window.ipc.postMessage('quit_app');
        return;
    }

    // Cmd+A to select all (but let default work in input fields)
    if (e.metaKey && e.key === 'a') {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
            return; // Let default select-all work in input fields
        }
        e.preventDefault();
        const content = document.getElementById('content');
        const range = document.createRange();
        range.selectNodeContents(content);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        return;
    }

    // Shift+Cmd+C to copy formatted (HTML)
    if (e.shiftKey && e.metaKey && e.key === 'c') {
        document.execCommand('copy');
        return;
    }

    // Cmd+C to copy markdown source (GitHub view) or plain text (terminal view)
    if (e.metaKey && e.key === 'c') {
        if (currentMode === 'github') {
            const markdown = extractMarkdownForSelection();
            if (markdown && window.ipc) {
                window.ipc.postMessage('clipboard:' + markdown);
                return;
            }
        }
        // Let default copy happen for terminal view or if no markdown found
        return;
    }

    // Cmd+Plus to increase font size
    if (e.metaKey && (e.key === '=' || e.key === '+')) {
        e.preventDefault();
        adjustFontSize(1);
        return;
    }

    // Cmd+Minus to decrease font size
    if (e.metaKey && e.key === '-') {
        e.preventDefault();
        adjustFontSize(-1);
        return;
    }

    // Cmd+0 to reset font size
    if (e.metaKey && e.key === '0') {
        e.preventDefault();
        resetFontSize();
        return;
    }

    // Escape to close search
    if (e.key === 'Escape') {
        closeSearch();
        return;
    }

    // Ignore other shortcuts if typing in input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    // Tab to toggle view (before modifier check)
    if (e.key === 'Tab') {
        e.preventDefault();
        const headingId = getCurrentHeadingId();
        setMode(currentMode === 'github' ? 'terminal' : 'github', headingId);
        return;
    }

    if (e.metaKey || e.ctrlKey || e.altKey) return;

    switch(e.key.toLowerCase()) {
        case 'c':
            toggleToc();
            break;
    }
});

// Search functionality
let searchMatches = [];
let currentMatchIndex = -1;

function openSearch() {
    const searchBar = document.getElementById('search-bar');
    searchBar.classList.remove('hidden');
    document.getElementById('search-input').focus();
}

function closeSearch() {
    const searchBar = document.getElementById('search-bar');
    searchBar.classList.add('hidden');
    clearHighlights();
    document.getElementById('search-input').value = '';
    document.getElementById('search-count').textContent = '';
}

function clearHighlights() {
    document.querySelectorAll('mark.search-highlight').forEach(mark => {
        const parent = mark.parentNode;
        parent.replaceChild(document.createTextNode(mark.textContent), mark);
        parent.normalize();
    });
    searchMatches = [];
    currentMatchIndex = -1;
}

function performSearch(query) {
    clearHighlights();
    if (!query || query.length < 2) {
        document.getElementById('search-count').textContent = '';
        return;
    }

    const activeView = currentMode === 'github' ? document.getElementById('github-view') : document.getElementById('terminal-view');
    const walker = document.createTreeWalker(activeView, NodeFilter.SHOW_TEXT, null, false);
    const textNodes = [];

    while (walker.nextNode()) {
        textNodes.push(walker.currentNode);
    }

    const regex = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');

    textNodes.forEach(node => {
        const text = node.textContent;
        if (regex.test(text)) {
            regex.lastIndex = 0;
            const fragment = document.createDocumentFragment();
            let lastIndex = 0;
            let match;

            while ((match = regex.exec(text)) !== null) {
                if (match.index > lastIndex) {
                    fragment.appendChild(document.createTextNode(text.slice(lastIndex, match.index)));
                }
                const mark = document.createElement('mark');
                mark.className = 'search-highlight';
                mark.textContent = match[0];
                fragment.appendChild(mark);
                lastIndex = regex.lastIndex;
            }

            if (lastIndex < text.length) {
                fragment.appendChild(document.createTextNode(text.slice(lastIndex)));
            }

            node.parentNode.replaceChild(fragment, node);
        }
    });

    searchMatches = Array.from(document.querySelectorAll('mark.search-highlight'));
    currentMatchIndex = searchMatches.length > 0 ? 0 : -1;
    updateSearchCount();
    highlightCurrentMatch();
}

function updateSearchCount() {
    const count = searchMatches.length;
    const current = currentMatchIndex + 1;
    document.getElementById('search-count').textContent = count > 0 ? `${current}/${count}` : 'No results';
}

function highlightCurrentMatch() {
    searchMatches.forEach((m, i) => {
        m.classList.toggle('current', i === currentMatchIndex);
    });
    if (searchMatches[currentMatchIndex]) {
        searchMatches[currentMatchIndex].scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
}

function searchNext() {
    if (searchMatches.length === 0) return;
    currentMatchIndex = (currentMatchIndex + 1) % searchMatches.length;
    updateSearchCount();
    highlightCurrentMatch();
}

function searchPrev() {
    if (searchMatches.length === 0) return;
    currentMatchIndex = (currentMatchIndex - 1 + searchMatches.length) % searchMatches.length;
    updateSearchCount();
    highlightCurrentMatch();
}

// Search input handler
document.getElementById('search-input').addEventListener('input', function(e) {
    performSearch(e.target.value);
});

document.getElementById('search-input').addEventListener('keydown', function(e) {
    if (e.key === 'Enter') {
        e.shiftKey ? searchPrev() : searchNext();
    }
});

function initCodeBlocks() {
    // GitHub view: add language labels to code blocks
    document.querySelectorAll('#github-view pre code').forEach((codeBlock) => {
        const classes = codeBlock.className.split(' ');
        let lang = '';
        for (const cls of classes) {
            if (cls.startsWith('language-')) {
                lang = cls.replace('language-', '');
                break;
            }
        }

        const pre = codeBlock.parentElement;
        if (!pre.querySelector('.code-header') && lang) {
            const header = document.createElement('div');
            header.className = 'code-header';
            header.textContent = lang;
            pre.insertBefore(header, codeBlock);
        }

        // Apply syntax highlighting
        if (typeof hljs !== 'undefined') {
            hljs.highlightElement(codeBlock);
        }
    });

    // Terminal view: custom markdown syntax highlighting
    highlightMarkdown();
}

function formatTable(tableLines) {
    // Parse table into cells
    const rows = tableLines.map(line => {
        const cells = line.split('|').slice(1, -1).map(c => c.trim());
        return cells;
    });

    if (rows.length < 2) return tableLines;

    // Find max width for each column
    const colWidths = [];
    for (const row of rows) {
        for (let i = 0; i < row.length; i++) {
            const cellLen = row[i].replace(/[-:]/g, m => m === '-' ? '-' : '').length;
            colWidths[i] = Math.max(colWidths[i] || 0, cellLen, 3);
        }
    }

    // Format each row
    return rows.map((row, rowIndex) => {
        const cells = row.map((cell, i) => {
            const width = colWidths[i] || 3;
            if (rowIndex === 1 && cell.match(/^[-:]+$/)) {
                // Separator row
                return '-'.repeat(width);
            }
            return cell.padEnd(width);
        });
        return '| ' + cells.join(' | ') + ' |';
    });
}

function highlightMarkdown() {
    const terminalView = document.getElementById('terminal-view');
    // Get raw text from initial content (stored in data attribute or parsed from escaped HTML)
    let text = terminalView.textContent;
    let html = '';
    let inCodeBlock = false;
    let codeBlockLang = '';
    let codeBlockLines = [];

    // Pre-process: format tables
    let lines = text.split('\n');
    let formattedLines = [];
    let tableBuffer = [];

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (line.trim().startsWith('|') && line.trim().endsWith('|')) {
            tableBuffer.push(line);
        } else {
            if (tableBuffer.length > 0) {
                formattedLines.push(...formatTable(tableBuffer));
                tableBuffer = [];
            }
            formattedLines.push(line);
        }
    }
    if (tableBuffer.length > 0) {
        formattedLines.push(...formatTable(tableBuffer));
    }

    lines = formattedLines;

    // Helper to create a line div with optional padding
    function makeLine(content, indent) {
        const style = indent > 0 ? ' style="padding-left:' + indent + 'ch"' : '';
        return '<div class="line"' + style + '>' + content + '</div>';
    }

    for (let i = 0; i < lines.length; i++) {
        let line = lines[i];

        // Code block start/end
        if (line.match(/^```/)) {
            if (!inCodeBlock) {
                inCodeBlock = true;
                codeBlockLang = line.slice(3).trim();
                html += makeLine('<span class="md-code-fence">' + escapeHtml(line) + '</span>', 0);
                codeBlockLines = [];
            } else {
                // End of code block - render collected content
                if (codeBlockLines.length > 0) {
                    let codeContent;
                    if (codeBlockLang && typeof hljs !== 'undefined') {
                        try {
                            const highlighted = hljs.highlight(codeBlockLines.join('\n'), { language: codeBlockLang, ignoreIllegals: true });
                            codeContent = highlighted.value;
                        } catch (e) {
                            codeContent = escapeHtml(codeBlockLines.join('\n'));
                        }
                    } else {
                        codeContent = escapeHtml(codeBlockLines.join('\n'));
                    }
                    html += '<div class="line md-code-block-wrapper"><pre>' + codeContent + '</pre></div>';
                }
                html += makeLine('<span class="md-code-fence">' + escapeHtml(line) + '</span>', 0);
                inCodeBlock = false;
                codeBlockLang = '';
            }
            continue;
        }

        if (inCodeBlock) {
            codeBlockLines.push(line);
            continue;
        }

        // Check for indentation before escaping
        const indentMatch = line.match(/^(\s+)/);
        const indent = indentMatch ? indentMatch[1].length : 0;
        const content = indent > 0 ? line.slice(indent) : line;

        let processed = escapeHtml(content);

        // Headings - add ID for navigation (use unescaped content for slug to match Rust)
        if (content.match(/^#{1,6}\s/)) {
            const headingText = content.replace(/^#+\s*/, '');
            const slug = slugify(headingText);
            html += makeLine('<span class="md-heading" id="' + slug + '">' + processed + '</span>', indent);
            continue;
        }

        // Horizontal rules
        if (processed.match(/^(-{3,}|\*{3,}|_{3,})$/)) {
            html += makeLine('<span class="md-hr">' + processed + '</span>', indent);
            continue;
        }

        // Blockquotes
        if (processed.match(/^&gt;\s?/)) {
            html += makeLine('<span class="md-blockquote">' + processed + '</span>', indent);
            continue;
        }

        // Table rows
        if (processed.match(/^\|.*\|$/)) {
            if (processed.match(/^\|[\s\-|]+\|$/)) {
                html += makeLine('<span class="md-table-sep">' + processed + '</span>', indent);
            } else {
                let tableLine = processed;
                tableLine = tableLine.replace(/\*\*(.+?)\*\*/g, '<span class="md-bold">&#42;&#42;$1&#42;&#42;</span>');
                tableLine = tableLine.replace(/__(.+?)__/g, '<span class="md-bold">&#95;&#95;$1&#95;&#95;</span>');
                tableLine = tableLine.replace(/\*(.+?)\*/g, '<span class="md-italic">&#42;$1&#42;</span>');
                tableLine = tableLine.replace(/(?<!\w)_(.+?)_(?!\w)/g, '<span class="md-italic">&#95;$1&#95;</span>');
                tableLine = highlightInlineCode(tableLine);
                tableLine = tableLine.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link">[$1]($2)</a>');
                html += makeLine('<span class="md-table">' + tableLine + '</span>', indent);
            }
            continue;
        }

        // List items
        processed = processed.replace(/^([-*+]|\d+\.)\s/, '<span class="md-list-marker">$1</span> ');

        // Inline formatting (use HTML entities for markers to prevent re-matching)
        processed = processed.replace(/\*\*(.+?)\*\*/g, '<span class="md-bold">&#42;&#42;$1&#42;&#42;</span>');
        processed = processed.replace(/__(.+?)__/g, '<span class="md-bold">&#95;&#95;$1&#95;&#95;</span>');
        processed = processed.replace(/\*(.+?)\*/g, '<span class="md-italic">&#42;$1&#42;</span>');
        processed = processed.replace(/(?<!\w)_(.+?)_(?!\w)/g, '<span class="md-italic">&#95;$1&#95;</span>');
        processed = highlightInlineCode(processed);
        processed = processed.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" class="md-link">[$1]($2)</a>');

        html += makeLine(processed, indent);
    }

    terminalView.innerHTML = html;
}

function highlightInlineCode(text) {
    // Parse backticks properly: count opening backticks, find matching closing sequence
    let result = '';
    let i = 0;
    while (i < text.length) {
        if (text[i] === '\u0060') {
            // Count consecutive backticks
            let backtickCount = 0;
            let start = i;
            while (i < text.length && text[i] === '\u0060') {
                backtickCount++;
                i++;
            }
            // Look for matching closing backticks
            const closer = '\u0060'.repeat(backtickCount);
            const closeIdx = text.indexOf(closer, i);
            if (closeIdx !== -1) {
                // Found matching closer
                const codeContent = text.slice(i, closeIdx);
                result += '<span class="md-code">' + closer + codeContent + closer + '</span>';
                i = closeIdx + backtickCount;
            } else {
                // No matching closer, output backticks as-is
                result += text.slice(start, i);
            }
        } else {
            result += text[i];
            i++;
        }
    }
    return result;
}

function escapeHtml(text) {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}

function slugify(text) {
    return text.toLowerCase()
        .split('')
        .map(c => /[a-z0-9]/.test(c) ? c : '-')
        .join('')
        .split('-')
        .filter(s => s.length > 0)
        .join('-');
}

// TOC scroll tracking
function updateTocHighlight() {
    const content = document.getElementById('content');
    const activeView = currentMode === 'github' ? '#github-view' : '#terminal-view';
    const headings = document.querySelectorAll(activeView + ' h1, ' + activeView + ' h2, ' + activeView + ' h3, ' + activeView + ' h4, ' + activeView + ' h5, ' + activeView + ' h6, ' + activeView + ' [id].md-heading');

    let currentHeading = null;
    const scrollTop = content.scrollTop;

    for (const heading of headings) {
        if (heading.offsetTop <= scrollTop + 100) {
            currentHeading = heading;
        } else {
            break;
        }
    }

    // Update TOC highlighting
    document.querySelectorAll('.toc-item').forEach(item => item.classList.remove('active'));

    if (currentHeading && currentHeading.id) {
        const tocItem = document.querySelector('.toc-item[onclick*="' + currentHeading.id + '"]');
        if (tocItem) {
            tocItem.classList.add('active');
        }
    }
}

// Save settings on window resize (debounced)
let resizeTimeout;
window.addEventListener('resize', function() {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(saveSettings, 500);
});

// Initialize
document.addEventListener('DOMContentLoaded', function() {
    initCodeBlocks();

    // Add scroll listener for TOC highlighting
    document.getElementById('content').addEventListener('scroll', updateTocHighlight);

    // Apply settings from initialSettings (already set at top of script)
    // Set view mode
    const content = document.getElementById('content');
    content.className = 'content ' + currentMode;
    document.getElementById('github-view').style.display = currentMode === 'github' ? 'block' : 'none';
    document.getElementById('terminal-view').style.display = currentMode === 'terminal' ? 'block' : 'none';

    // Show TOC if enabled
    if (tocVisible) {
        document.getElementById('toc').classList.remove('hidden');
    }

    // Apply font size
    applyFontSize();

    // Initial highlight
    updateTocHighlight();
});
