// ============================================================================
// STATE & CONSTANTS
// ============================================================================

let currentMode = initialSettings.view_mode || 'github';
let tocVisible = initialSettings.toc_visible || false;
let fontSizeLevel = initialSettings.font_size_level || 0;
let currentTheme = initialSettings.theme || 'dark';
let cellsCollapsedPref = initialSettings.cells_collapsed || false;
let outputWrapped = initialSettings.output_wrapped || false;
const currentExtension = initialSettings.extension || 'md';
const isNotebook = currentExtension === 'ipynb';
const TOC_WIDTH = 200;
const BASE_FONT_SIZE = 15;
const TERMINAL_BASE_SIZE = 11;

// ============================================================================
// SETTINGS & PREFERENCES
// ============================================================================

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
            font_size_level: fontSizeLevel,
            theme: currentTheme,
            cells_collapsed: cellsCollapsed,
            output_wrapped: outputWrapped
        };
        // Include extension in message format: save_settings:ext:{json}
        window.ipc.postMessage('save_settings:' + currentExtension + ':' + JSON.stringify(settings));
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
    const terminalView = document.getElementById('terminal-view');
    if (terminalView) {
        terminalView.style.fontSize = (TERMINAL_BASE_SIZE * scale) + 'px';
    }
}

function setTheme(theme) {
    currentTheme = theme;
    document.body.classList.toggle('light', theme === 'light');
    saveSettings();
}

function toggleTheme() {
    setTheme(currentTheme === 'dark' ? 'light' : 'dark');
}

// ============================================================================
// VIEW SWITCHING & NAVIGATION
// ============================================================================

function getActiveViewSelector() {
    if (isNotebook) {
        return '#notebook-view';
    }
    return currentMode === 'github' ? '#github-view' : '#terminal-view';
}

function getCurrentHeadingId() {
    const content = document.getElementById('content');
    const activeView = getActiveViewSelector();
    const headings = document.querySelectorAll(activeView + ' h1, ' + activeView + ' h2, ' + activeView + ' h3, ' + activeView + ' h4, ' + activeView + ' h5, ' + activeView + ' h6, ' + activeView + ' [id].md-heading');

    let currentHeading = null;
    // Clamp scrollTop to valid range (handles macOS bounce effect)
    const maxScroll = Math.max(0, content.scrollHeight - content.clientHeight);
    const scrollTop = Math.max(0, Math.min(content.scrollTop, maxScroll));

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

    // Hide content while repositioning to prevent flash
    content.style.visibility = 'hidden';
    content.className = 'content ' + mode;

    // Toggle views
    document.getElementById('github-view').style.display = mode === 'github' ? 'block' : 'none';
    document.getElementById('terminal-view').style.display = mode === 'terminal' ? 'block' : 'none';

    // Reset scroll first
    content.scrollTop = 0;

    // Wait for browser to render the new view before scrolling
    requestAnimationFrame(() => {
        requestAnimationFrame(() => {
            if (scrollToId) {
                const newView = mode === 'github' ? '#github-view' : '#terminal-view';
                const el = document.querySelector(newView + ' #' + CSS.escape(scrollToId));
                if (el) {
                    // Calculate target scroll position
                    const targetTop = el.offsetTop;
                    // Only scroll if content is actually scrollable
                    const maxScroll = content.scrollHeight - content.clientHeight;
                    if (maxScroll > 0) {
                        content.scrollTop = Math.min(targetTop, maxScroll);
                    }
                }
            }
            // Show content after positioning
            content.style.visibility = '';
        });
    });

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
    const activeView = getActiveViewSelector();
    const el = document.querySelector(activeView + ' #' + CSS.escape(slug));
    if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
}

// ============================================================================
// COPY HANDLING (Cmd+C)
// ============================================================================

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

// ============================================================================
// KEYBOARD SHORTCUTS
// ============================================================================

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

    // Cmd+C to copy markdown source (GitHub view) or formatted (notebooks)
    if (e.metaKey && e.key === 'c') {
        if (isNotebook) {
            // For notebooks, copy formatted (same as execCommand)
            document.execCommand('copy');
            return;
        }
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

    // Tab to toggle view (markdown only, before modifier check)
    if (e.key === 'Tab' && !isNotebook) {
        e.preventDefault();
        const headingId = getCurrentHeadingId();
        setMode(currentMode === 'github' ? 'terminal' : 'github', headingId);
        return;
    }

    // C to toggle collapse all cells (notebook only)
    if (e.key === 'c' && isNotebook) {
        e.preventDefault();
        toggleAllCells();
        return;
    }

    // W to toggle output wrap (notebook only)
    if (e.key === 'w' && isNotebook) {
        e.preventDefault();
        toggleOutputWrap();
        return;
    }

    if (e.metaKey || e.ctrlKey || e.altKey) return;

    switch(e.key.toLowerCase()) {
        case 't':
            toggleToc();
            break;
        case 'd':
            toggleTheme();
            break;
    }
});

// ============================================================================
// SEARCH
// ============================================================================

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

    const activeView = document.querySelector(getActiveViewSelector());
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

// ============================================================================
// NOTEBOOK: CELL COLLAPSE, OUTPUT WRAP & IMAGE EXPAND
// ============================================================================

let cellsCollapsed = cellsCollapsedPref;  // Initialize from saved preference

function checkAndSyncGlobalCollapseState() {
    // Check if all cells are in the same state, and if so, sync the global state
    const cells = document.querySelectorAll('.nb-cell');
    if (cells.length === 0) return;

    const allCollapsed = Array.from(cells).every(c => c.classList.contains('collapsed'));
    const allExpanded = Array.from(cells).every(c => !c.classList.contains('collapsed'));

    if (allCollapsed) {
        cellsCollapsed = true;
    } else if (allExpanded) {
        cellsCollapsed = false;
    }
    // If mixed state, don't change the global state
}

function toggleCellCollapse(cellEl) {
    const scrollY = window.scrollY;
    cellEl.classList.toggle('collapsed');
    window.scrollTo(0, scrollY); // Preserve scroll position

    // Check if all cells now match, and sync global state
    checkAndSyncGlobalCollapseState();
}

function toggleAllCells() {
    const scrollY = window.scrollY;
    const cells = document.querySelectorAll('.nb-cell');
    if (cells.length === 0) return;

    // Toggle based on tracked global state
    cellsCollapsed = !cellsCollapsed;
    cells.forEach(c => c.classList.toggle('collapsed', cellsCollapsed));
    window.scrollTo(0, scrollY); // Preserve scroll position
    saveSettings();
}

function toggleOutputWrap() {
    outputWrapped = !outputWrapped;
    document.body.classList.toggle('output-wrapped', outputWrapped);
    saveSettings();
}

function applyOutputWrap() {
    document.body.classList.toggle('output-wrapped', outputWrapped);
}

function expandFigure(img) {
    // Create overlay
    const overlay = document.createElement('div');
    overlay.className = 'nb-figure-overlay visible';
    overlay.onclick = function() { closeFigureOverlay(); };

    // Clone the image
    const expandedImg = img.cloneNode();
    expandedImg.className = 'nb-figure-expanded';
    overlay.appendChild(expandedImg);

    document.body.appendChild(overlay);

    // Close on escape key
    document.addEventListener('keydown', handleFigureEscape);
}

function closeFigureOverlay() {
    const overlay = document.querySelector('.nb-figure-overlay');
    if (overlay) {
        overlay.remove();
        document.removeEventListener('keydown', handleFigureEscape);
    }
}

function handleFigureEscape(e) {
    if (e.key === 'Escape') {
        closeFigureOverlay();
    }
}

// ============================================================================
// NOTEBOOK: OUTPUT TRUNCATION
// ============================================================================

function requestMoreLines(cellIdx, outputIdx, amount) {
    if (window.ipc) {
        window.ipc.postMessage(`get_output_lines:${cellIdx}:${outputIdx}:${amount}`);
    }
}

// Called from Rust via evaluate_script
function receiveOutputLines(cellIdx, outputIdx, linesHtml, hiddenRemaining, isComplete) {
    const output = document.querySelector(
        `.nb-output[data-cell-idx="${cellIdx}"][data-output-idx="${outputIdx}"]`
    );
    if (!output) return;

    const headDiv = output.querySelector('.nb-output-head');
    const truncatedDiv = output.querySelector('.nb-output-truncated');
    const tailDiv = output.querySelector('.nb-output-tail');

    if (isComplete) {
        // Show all remaining lines, remove truncation UI
        if (headDiv && linesHtml) {
            headDiv.innerHTML += '\n' + linesHtml;
        }
        if (truncatedDiv) truncatedDiv.remove();
        if (tailDiv && headDiv) {
            headDiv.innerHTML += '\n' + tailDiv.innerHTML;
            tailDiv.remove();
        }
    } else {
        // Append lines, update count
        if (headDiv && linesHtml) {
            headDiv.innerHTML += '\n' + linesHtml;
        }
        if (truncatedDiv) {
            const infoSpan = truncatedDiv.querySelector('.nb-truncated-info');
            if (infoSpan) {
                infoSpan.textContent = `${hiddenRemaining} lines hidden`;
            }
        }
    }
}

// ============================================================================
// MARKDOWN: CODE BLOCKS & TERMINAL VIEW
// ============================================================================

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

    // GitHub view: add click-to-expand for images
    document.querySelectorAll('#github-view img').forEach(img => {
        img.addEventListener('click', function() {
            expandFigure(this);
        });
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
    let inComment = false;

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

        // HTML comments - handle multi-line (check before code blocks so comments containing code aren't highlighted)
        if (inComment) {
            if (line.includes('-->')) {
                // Comment ends on this line
                inComment = false;
            }
            html += makeLine('<span class="md-comment">' + escapeHtml(line) + '</span>', 0);
            continue;
        }

        // Check for comment start (before code blocks)
        if (line.includes('<!--')) {
            if (!line.includes('-->')) {
                // Comment starts but doesn't end on this line
                inComment = true;
            }
            html += makeLine('<span class="md-comment">' + escapeHtml(line) + '</span>', 0);
            continue;
        }

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

// ============================================================================
// FILE RELOAD (Live refresh on file changes)
// ============================================================================

/**
 * Called from Rust when the file changes.
 * Replaces content while preserving scroll position.
 *
 * @param {string} newHtml - New rendered HTML content
 * @param {string} newTocHtml - New TOC HTML
 * @param {boolean} isNotebookReload - Whether this is a notebook file
 * @param {string} newTerminalHtml - For markdown: new terminal view content (optional)
 */
function reloadContent(newHtml, newTocHtml, isNotebookReload, newTerminalHtml) {
    const content = document.getElementById('content');
    const scrollTop = content.scrollTop;
    const scrollHeight = content.scrollHeight;
    const scrollRatio = scrollHeight > 0 ? scrollTop / scrollHeight : 0;

    // Update main content
    if (isNotebookReload) {
        const notebookView = document.getElementById('notebook-view');
        if (notebookView) {
            notebookView.innerHTML = newHtml;
            // Re-initialize notebook features
            initNotebook();
            // Re-render KaTeX math in markdown cells
            if (typeof renderMathInElement !== 'undefined') {
                notebookView.querySelectorAll('.nb-markdown-cell').forEach(cell => {
                    renderMathInElement(cell, {
                        delimiters: [
                            {left: '$$', right: '$$', display: true},
                            {left: '$', right: '$', display: false}
                        ],
                        throwOnError: false
                    });
                });
            }
        }
    } else {
        const githubView = document.getElementById('github-view');
        const terminalView = document.getElementById('terminal-view');
        if (githubView) {
            githubView.innerHTML = newHtml;
            // Re-render KaTeX math
            if (typeof renderMathInElement !== 'undefined') {
                renderMathInElement(githubView, {
                    delimiters: [
                        {left: '$$', right: '$$', display: true},
                        {left: '$', right: '$', display: false}
                    ],
                    throwOnError: false
                });
            }
        }
        if (terminalView && newTerminalHtml) {
            terminalView.innerHTML = newTerminalHtml;
        }
        // Re-initialize markdown features
        initCodeBlocks();
    }

    // Update TOC
    const toc = document.getElementById('toc');
    if (toc) {
        toc.innerHTML = newTocHtml;
    }

    // Restore scroll position (use ratio as fallback if content height changed significantly)
    requestAnimationFrame(() => {
        const newScrollHeight = content.scrollHeight;
        // If content height is similar, use absolute position; otherwise use ratio
        const oldHeightEstimate = scrollRatio > 0 ? scrollTop / scrollRatio : 0;
        if (Math.abs(newScrollHeight - oldHeightEstimate) < 100 || scrollRatio === 0) {
            content.scrollTop = scrollTop;
        } else {
            content.scrollTop = scrollRatio * newScrollHeight;
        }

        // Update TOC highlight
        updateTocHighlight();
    });
}

// ============================================================================
// TOC & INITIALIZATION
// ============================================================================

function updateTocHighlight() {
    const content = document.getElementById('content');
    const activeView = getActiveViewSelector();
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
    if (isNotebook) {
        // Notebook mode initialization
        initNotebook();
    } else {
        // Markdown mode initialization
        initCodeBlocks();

        // Set view mode
        const content = document.getElementById('content');
        content.className = 'content ' + currentMode;
        document.getElementById('github-view').style.display = currentMode === 'github' ? 'block' : 'none';
        document.getElementById('terminal-view').style.display = currentMode === 'terminal' ? 'block' : 'none';
    }

    // Add scroll listener for TOC highlighting
    document.getElementById('content').addEventListener('scroll', updateTocHighlight);

    // Show TOC if enabled
    if (tocVisible) {
        document.getElementById('toc').classList.remove('hidden');
    }

    // Apply font size
    applyFontSize();

    // Apply theme
    if (currentTheme === 'light') {
        document.body.classList.add('light');
    }

    // Initial highlight
    updateTocHighlight();

    // Reveal content after initialization (hidden in template to prevent flash)
    document.getElementById('content').style.visibility = '';
});

function initNotebook() {
    const notebookView = document.getElementById('notebook-view');
    if (!notebookView) return;

    // Apply syntax highlighting to notebook code cells
    notebookView.querySelectorAll('pre code').forEach((codeBlock) => {
        if (typeof hljs !== 'undefined') {
            hljs.highlightElement(codeBlock);
        }
    });

    // Wire up collapse buttons
    const buttons = notebookView.querySelectorAll('.nb-collapse-btn');
    buttons.forEach(btn => {
        btn.addEventListener('click', function(e) {
            e.preventDefault();
            e.stopPropagation();
            const cell = this.closest('.nb-cell');
            if (cell) {
                toggleCellCollapse(cell);
            }
        });
    });

    // Wire up figure expansion
    notebookView.querySelectorAll('.nb-figure').forEach(img => {
        img.addEventListener('click', function() {
            expandFigure(this);
        });
    });

    // Wire up output truncation buttons
    notebookView.querySelectorAll('.nb-show-more').forEach(btn => {
        btn.addEventListener('click', function(e) {
            e.preventDefault();
            const output = this.closest('.nb-output');
            if (output) {
                const cellIdx = parseInt(output.dataset.cellIdx);
                const outputIdx = parseInt(output.dataset.outputIdx);
                requestMoreLines(cellIdx, outputIdx, this.dataset.amount || '50');
            }
        });
    });

    notebookView.querySelectorAll('.nb-show-all').forEach(btn => {
        btn.addEventListener('click', function(e) {
            e.preventDefault();
            const output = this.closest('.nb-output');
            if (output) {
                const cellIdx = parseInt(output.dataset.cellIdx);
                const outputIdx = parseInt(output.dataset.outputIdx);
                requestMoreLines(cellIdx, outputIdx, 'all');
            }
        });
    });

    // Apply saved preferences
    if (cellsCollapsed) {
        const cells = document.querySelectorAll('.nb-cell');
        cells.forEach(c => c.classList.add('collapsed'));
    }
    applyOutputWrap();
}
