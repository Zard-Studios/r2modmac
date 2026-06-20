import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { useAppStore } from '../../store/useAppStore';
import { censorPath, uncensorPath } from '../../utils/pathCensorUtils';
import { CensoredInput } from '../ui/PathCensor';
import { revealInFileManagerLabel } from '../../utils/platformUtils';

import type { ConfigFileInfo } from '../../tauriAdapter';

// ─── Config parsing types (mirrors r2modman ConfigUtils) ─────────────────────

type DisplayType = 'text' | 'boolean' | 'single-select' | 'multi-select';

interface CommentLine {
    isDescription: boolean; // ## → true, # → false
    displayValue: string;
    rawValue: string;
}

interface CfgEntry {
    key: string;
    value: string;
    cachedValue: string; // value as loaded from disk (dirty detection)
    comments: CommentLine[];
    displayType: DisplayType;
}

interface CfgSection {
    name: string;
    entries: CfgEntry[];
}

interface ParsedCfg {
    sections: CfgSection[];
}

// ─── Parsing helpers ─────────────────────────────────────────────────────────

function buildComments(rawLines: string[]): CommentLine[] {
    return rawLines
        .filter((l) => l.trim().substring(1).length > 0)
        .map((l): CommentLine => {
            if (l.trim().startsWith('##')) {
                return {
                    isDescription: true,
                    displayValue: l.trim().substring(2).trim(),
                    rawValue: l.trim(),
                };
            }
            return {
                isDescription: false,
                displayValue: l.trim().substring(1).trim(),
                rawValue: l.trim(),
            };
        });
}

function determineDisplayType(comments: string[]): DisplayType {
    if (comments.some((c) => c.includes('# Setting type: Boolean'))) return 'boolean';
    if (comments.some((c) => c.includes('# Multiple values can be set at the same time')))
        return 'multi-select';
    if (comments.some((c) => c.includes('# Acceptable values:'))) return 'single-select';
    return 'text';
}

function getSelectOptions(entry: CfgEntry): string[] {
    if (entry.displayType === 'boolean') return ['true', 'false'];
    const line = entry.comments.find((c) => c.rawValue.includes('# Acceptable values:'));
    if (!line) return [];
    return line.rawValue
        .substring('# Acceptable values: '.length)
        .split(',')
        .map((v) => v.trim())
        .sort();
}

function parseConfigEntries(lines: string[]): CfgEntry[] {
    const entries: CfgEntry[] = [];
    let pendingComments: string[] = [];
    for (const line of lines) {
        if (line.trim().startsWith('#')) {
            pendingComments.push(line);
        } else if (line.trim().length > 0 && line.includes('=')) {
            const eqIdx = line.indexOf('=');
            const key = line.substring(0, eqIdx).trim();
            const value = line.substring(eqIdx + 1).trim();
            const comments = buildComments(pendingComments);
            entries.push({
                key,
                value,
                cachedValue: value,
                comments,
                displayType: determineDisplayType(pendingComments),
            });
            pendingComments = [];
        }
    }
    return entries;
}

function parseCfg(raw: string): ParsedCfg {
    const lines = raw.split(/\r?\n/);
    const sections: CfgSection[] = [];
    let currentSection: CfgSection | null = null;
    let sectionLines: string[] = [];

    for (const line of lines) {
        if (line.trim().startsWith('[') && line.trim().endsWith(']')) {
            // Flush previous section entries
            if (currentSection) {
                currentSection.entries = parseConfigEntries(sectionLines);
                sectionLines = [];
            }
            const name = line.trim().slice(1, -1).trim();
            currentSection = { name, entries: [] };
            sections.push(currentSection);
        } else if (line.trim().length > 0) {
            sectionLines.push(line.trim());
        }
    }
    // Flush last section
    if (currentSection) {
        currentSection.entries = parseConfigEntries(sectionLines);
    }

    return { sections };
}

function serializeCfg(parsed: ParsedCfg): string {
    let out = '';
    for (const section of parsed.sections) {
        if (section.name.trim()) {
            out += `[${section.name}]\n\n`;
        }
        for (const entry of section.entries) {
            const comments = entry.comments.map((c) => c.rawValue).join('\n');
            if (comments) out += `${comments}\n`;
            out += `${entry.key} = ${entry.value}\n\n`;
        }
    }
    return out;
}

// ─── File-type helpers ────────────────────────────────────────────────────────

function isCfgFile(name: string): boolean {
    return name.toLowerCase().endsWith('.cfg');
}

function FileExtIcon({ name, className }: { name: string; className?: string }) {
    const ext = name.split('.').pop()?.toLowerCase() ?? '';
    const isCfg = ext === 'cfg';
    const isJson = ext === 'json';
    const isText = ['txt', 'md', 'log'].includes(ext);
    const isYaml = ['yml', 'yaml'].includes(ext);
    const isIni = ext === 'ini';

    return (
        <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            {isCfg ? (
                // gear / settings icon — unmistakable for config
                <>
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                </>
            ) : isJson ? (
                <>
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <path d="M9 13l-2 2 2 2" /><path d="M15 13l2 2-2 2" />
                </>
            ) : isIni ? (
                <>
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <line x1="10" y1="13" x2="10" y2="17" /><line x1="14" y1="13" x2="14" y2="17" />
                </>
            ) : isYaml ? (
                <>
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <path d="M8 13l2 3 2-3" /><path d="M14 13l2 3 2-3" />
                </>
            ) : isText ? (
                <>
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <line x1="16" y1="13" x2="8" y2="13" /><line x1="16" y1="17" x2="8" y2="17" /><line x1="10" y1="9" x2="8" y2="9" />
                </>
            ) : (
                <>
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                    <line x1="16" y1="13" x2="8" y2="13" /><line x1="16" y1="17" x2="8" y2="17" />
                </>
            )}
        </svg>
    );
}

// ─── Tree-building helpers ────────────────────────────────────────────────────

interface TreeNode {
    /** Display label (folder segment name) */
    label: string;
    /** Full path key (all segments joined) */
    key: string;
    /** Depth in the tree (root children = 0) */
    depth: number;
    /** Mod icon URL — only set on the "top-level mod" folder */
    iconUrl?: string;
    /** Whether this node was matched as a top-level mod folder */
    isModRoot: boolean;
    /** Direct files in this folder (not recursive) */
    files: ConfigFileInfo[];
    /** Sub-folders */
    children: TreeNode[];
}

/** Normalize a folder name like "Author-ModName-1.0.0" → "authormodname" for fuzzy matching */
function normalizeName(s: string): string {
    return s.toLowerCase().replace(/[^a-z0-9]/g, '');
}

const SYSTEM_FOLDERS = new Set([
    'config',
    'plugins',
    'patchers',
    'bepinex',
    'mods',
    'core',
    'cache',
    'monomod',
    'interop',
    'unity-libs',
    'doorstop_libs',
    'translation',
    'game root',
    '__root__',
    'bin',
    'local',
    'obsolete',
    'temp',
    'tmp',
    'managed',
    'resources'
]);

function findMatchedModForSegment(
    segment: string,
    mods: Array<{ fullName: string; iconUrl?: string; displayName?: string }>
) {
    if (SYSTEM_FOLDERS.has(segment.toLowerCase())) return undefined;
    const normCandidate = normalizeName(segment);
    if (!normCandidate) return undefined;
    return mods.find((m) => {
        const normFull = normalizeName(m.fullName);
        const nameParts = m.fullName.split('-');
        const modNamePart = nameParts.length >= 2 ? normalizeName(nameParts[1]) : '';
        return (
            normFull === normCandidate ||
            normFull.includes(normCandidate) ||
            normCandidate.includes(normFull) ||
            (modNamePart.length >= 3 && normCandidate.includes(modNamePart))
        );
    });
}

/**
 * Build a recursive tree from config files.
 * Each file's relative_path segments become nested TreeNodes.
 * Mod icons are attached to the highest non-system folder that matches a mod.
 */
function buildTree(
    files: ConfigFileInfo[],
    mods: Array<{ fullName: string; iconUrl?: string; displayName?: string }>,
    search: string,
): TreeNode[] {
    const lowSearch = search.toLowerCase();

    // Filter by search
    const filtered = files.filter((f) =>
        !lowSearch ||
        f.name.toLowerCase().includes(lowSearch) ||
        f.relative_path.toLowerCase().includes(lowSearch)
    );

    // Root children map: key → TreeNode
    const rootMap = new Map<string, TreeNode>();

    const getOrCreate = (
        map: Map<string, TreeNode>,
        parentKey: string,
        segment: string,
        depth: number,
        inheritedIconUrl?: string,
    ): TreeNode => {
        const key = parentKey ? `${parentKey}/${segment}` : segment;
        if (!map.has(key)) {
            // Try to match this segment to a mod (only at non-system segments)
            const matched = !inheritedIconUrl ? findMatchedModForSegment(segment, mods) : undefined;
            map.set(key, {
                label: segment,
                key,
                depth,
                iconUrl: matched?.iconUrl ?? inheritedIconUrl,
                isModRoot: !!matched,
                files: [],
                children: [],
            });
        }
        return map.get(key)!;
    };

    for (const f of filtered) {
        const parts = f.relative_path.split('/');
        const dirParts = parts.slice(0, -1);

        if (dirParts.length === 0) {
            // File at true root — put in a synthetic root bucket
            const rootNode = getOrCreate(rootMap, '', '__root__', 0);
            rootNode.label = 'Game Root';
            rootNode.files.push(f);
            continue;
        }

        // Walk down the path, creating/reusing nodes at each level
        let currentMap = rootMap;
        let parentKey = '';
        let inheritedIconUrl: string | undefined;
        for (let i = 0; i < dirParts.length; i++) {
            const segment = dirParts[i];
            
            // If this is a system folder, clear inherited icon so child mods inside it can resolve their own icons
            if (SYSTEM_FOLDERS.has(segment.toLowerCase())) {
                inheritedIconUrl = undefined;
            }

            const node = getOrCreate(currentMap, parentKey, segment, i, inheritedIconUrl);
            // Propagate mod icon to child folders once we have one
            if (!inheritedIconUrl && node.iconUrl) inheritedIconUrl = node.iconUrl;
            parentKey = node.key;

            if (i === dirParts.length - 1) {
                // Leaf folder: attach the file here
                node.files.push(f);
            } else {
                // Ensure child map entry exists for the next segment
                if (!currentMap.has(node.key)) currentMap.set(node.key, node);
                // Move into the children of this node
                const childrenMap = new Map<string, TreeNode>();
                for (const c of node.children) childrenMap.set(c.key, c);
                
                let nextInheritedIconUrl = inheritedIconUrl;
                if (SYSTEM_FOLDERS.has(dirParts[i + 1].toLowerCase())) {
                    nextInheritedIconUrl = undefined;
                }

                const childNode = getOrCreate(childrenMap, parentKey, dirParts[i + 1], i + 1, nextInheritedIconUrl);
                if (!node.children.find(c => c.key === childNode.key)) {
                    node.children.push(childNode);
                }
                currentMap = childrenMap;
            }
        }
    }

    // Sort helper: config first, then alphabetical
    const sortNodes = (nodes: TreeNode[]): TreeNode[] => {
        return nodes
            .sort((a, b) => {
                const aIsConfig = a.key.toLowerCase().includes('bepinex/config') || a.label.toLowerCase() === 'config';
                const bIsConfig = b.key.toLowerCase().includes('bepinex/config') || b.label.toLowerCase() === 'config';
                if (aIsConfig && !bIsConfig) return -1;
                if (bIsConfig && !aIsConfig) return 1;
                return a.label.localeCompare(b.label);
            })
            .map(n => ({ ...n, children: sortNodes(n.children) }));
    };

    return sortNodes(Array.from(rootMap.values()));
}

/** Count all files recursively in a TreeNode */
function countFiles(node: TreeNode): number {
    return node.files.length + node.children.reduce((s, c) => s + countFiles(c), 0);
}


// ─── Sub-components ───────────────────────────────────────────────────────────

interface EntryEditorProps {
    entry: CfgEntry;
    onChange: (newValue: string) => void;
}

function EntryEditor({ entry, onChange }: EntryEditorProps) {
    const descriptionLines = entry.comments.filter((c) => c.isDescription);
    const metaLines = entry.comments.filter((c) => !c.isDescription);

    return (
        <div className="py-3 border-b border-gray-700/50 last:border-0">
            {/* Description */}
            {descriptionLines.length > 0 && (
                <p className="text-xs text-gray-400 mb-1 italic leading-relaxed">
                    {descriptionLines.map((c) => c.displayValue).join(' ')}
                </p>
            )}

            <div className="flex items-center gap-3">
                <label className="text-sm font-medium text-gray-200 min-w-0 flex-1 truncate" title={entry.key}>
                    {entry.key}
                </label>

                {/* Value Widget */}
                <div className="flex-shrink-0 w-56">
                    {entry.displayType === 'boolean' ? (
                        <div className="flex justify-end">
                            <button
                                onClick={() => onChange(entry.value === 'true' ? 'false' : 'true')}
                                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                                    entry.value === 'true' ? 'bg-blue-600' : 'bg-gray-600'
                                }`}
                                role="switch"
                                aria-checked={entry.value === 'true'}
                            >
                                <span
                                    className={`inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${
                                        entry.value === 'true' ? 'translate-x-6' : 'translate-x-1'
                                    }`}
                                />
                            </button>
                        </div>
                    ) : entry.displayType === 'single-select' ? (
                        <select
                            value={entry.value}
                            onChange={(e) => onChange(e.target.value)}
                            className="w-full bg-gray-700 border border-gray-600 rounded-lg px-2 py-1 text-white text-sm focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                        >
                            {getSelectOptions(entry).map((opt) => (
                                <option key={opt} value={opt}>
                                    {opt}
                                </option>
                            ))}
                        </select>
                    ) : entry.displayType === 'multi-select' ? (
                        <CensoredInput
                            value={entry.value}
                            onChange={onChange}
                            placeholder="value1, value2"
                            className="w-full bg-gray-700 border border-gray-600 rounded-lg px-2 py-1 text-white text-sm font-mono focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                        />
                    ) : (
                        <CensoredInput
                            value={entry.value}
                            onChange={onChange}
                            className="w-full bg-gray-700 border border-gray-600 rounded-lg px-2 py-1 text-white text-sm font-mono focus:outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500"
                        />
                    )}
                </div>
            </div>

            {/* Acceptable values hint */}
            {entry.displayType === 'single-select' && (
                <p className="text-[10px] text-gray-500 mt-1 pl-0">
                    Options:{' '}
                    {getSelectOptions(entry).join(', ')}
                </p>
            )}
            {metaLines.length > 0 && entry.displayType === 'text' && (
                <p className="text-[10px] text-gray-500 mt-0.5">
                    {metaLines.map((c) => c.displayValue).join(' · ')}
                </p>
            )}
        </div>
    );
}

// ─── JSON Linter Helpers ──────────────────────────────────────────────────────



function findJsonErrorLocation(json: string): { line: number; col: number; message: string } {
    let index = 0;
    let line = 1;
    let col = 1;

    const peek = () => json[index];
    const next = () => {
        const char = json[index++];
        if (char === '\n') {
            line++;
            col = 1;
        } else {
            col++;
        }
        return char;
    };

    const skipWhitespace = () => {
        while (index < json.length) {
            const c = peek();
            if (c === ' ' || c === '\t' || c === '\n' || c === '\r') {
                next();
            } else if (c === '/' && json[index + 1] === '/') {
                // Comment //
                const startLine = line;
                const startCol = col;
                next(); next();
                while (index < json.length && peek() !== '\n') {
                    next();
                }
                throw { line: startLine, col: startCol, message: "Comments are not allowed in JSON standard (//)" };
            } else if (c === '/' && json[index + 1] === '*') {
                // Comment /*
                const startLine = line;
                const startCol = col;
                next(); next();
                while (index < json.length && !(peek() === '*' && json[index + 1] === '/')) {
                    next();
                }
                if (index < json.length) {
                    next(); next();
                }
                throw { line: startLine, col: startCol, message: "Comments are not allowed in JSON standard (/* */)" };
            } else {
                break;
            }
        }
    };

    const parseString = () => {
        const startLine = line;
        const startCol = col;
        const quote = next(); // consume opening quote (' or ")
        if (quote === "'") {
            throw { line: startLine, col: startCol, message: "Strings in JSON must be enclosed in double quotes (\"), not single quotes (')" };
        }
        while (index < json.length) {
            const c = peek();
            if (c === '"') {
                next();
                return true;
            }
            if (c === '\\') {
                next();
                next();
            } else {
                next();
            }
        }
        throw { line: startLine, col: startCol, message: "Unterminated string" };
    };

    const parseValue = () => {
        skipWhitespace();
        const c = peek();
        if (!c) {
            throw { line, col, message: "Unexpected end of JSON input" };
        }
        if (c === '"' || c === "'") {
            parseString();
        } else if (c === '{') {
            parseObject();
        } else if (c === '[') {
            parseArray();
        } else if (c === '-' || (c >= '0' && c <= '9')) {
            let numStr = '';
            while (index < json.length) {
                const char = peek();
                if ((char >= '0' && char <= '9') || char === '.' || char === '-' || char === '+' || char === 'e' || char === 'E') {
                    numStr += next();
                } else {
                    break;
                }
            }
            if (isNaN(Number(numStr))) {
                throw { line, col: col - numStr.length, message: `Invalid number format: "${numStr}"` };
            }
        } else if (json.substring(index, index + 4) === 'true') {
            index += 4; col += 4;
        } else if (json.substring(index, index + 5) === 'false') {
            index += 5; col += 5;
        } else if (json.substring(index, index + 4) === 'null') {
            index += 4; col += 4;
        } else {
            throw { line, col, message: `Unexpected token "${c}"` };
        }
    };

    const parseObject = () => {
        const startLine = line;
        const startCol = col;
        next(); // consume '{'
        skipWhitespace();
        if (peek() === '}') {
            next();
            return;
        }
        while (index < json.length) {
            skipWhitespace();
            const c = peek();
            if (c !== '"') {
                if (c === "'") {
                    throw { line, col, message: "Keys in JSON must be enclosed in double quotes (\"), not single quotes (')" };
                }
                if (c && /[a-zA-Z0-9_$]/.test(c)) {
                    throw { line, col, message: `Keys in JSON must be double-quoted. Found unquoted key starting with "${c}"` };
                }
                throw { line, col, message: `Expected double-quoted property name, got "${c || ''}"` };
            }
            parseString();
            skipWhitespace();
            if (peek() !== ':') {
                throw { line, col, message: "Colon expected" };
            }
            next(); // consume ':'
            parseValue();
            skipWhitespace();
            const nextC = peek();
            if (nextC === '}') {
                next();
                return;
            }
            if (nextC === ',') {
                next();
                skipWhitespace();
                if (peek() === '}') {
                    throw { line, col, message: "Trailing comma inside object" };
                }
            } else {
                throw { line, col, message: "Expected comma" };
            }
        }
        throw { line: startLine, col: startCol, message: "Unterminated object (missing closing \"}\")" };
    };

    const parseArray = () => {
        const startLine = line;
        const startCol = col;
        next(); // consume '['
        skipWhitespace();
        if (peek() === ']') {
            next();
            return;
        }
        while (index < json.length) {
            parseValue();
            skipWhitespace();
            const c = peek();
            if (c === ']') {
                next();
                return;
            }
            if (c === ',') {
                next();
                skipWhitespace();
                if (peek() === ']') {
                    throw { line, col, message: "Trailing comma inside array" };
                }
            } else {
                throw { line, col, message: "Expected comma" };
            }
        }
        throw { line: startLine, col: startCol, message: "Unterminated array (missing closing \"]\")" };
    };

    try {
        parseValue();
        skipWhitespace();
        if (index < json.length) {
            throw { line, col, message: `Unexpected extra characters after JSON value: "${peek()}"` };
        }
        return { line: 1, col: 1, message: "Valid JSON" };
    } catch (e: any) {
        return e;
    }
}

interface JSONErrorDetail {
    message: string;
    line: number;
    column?: number;
}

function lintJSONAll(content: string): JSONErrorDetail[] {
    if (!content.trim()) return [];

    // ── Pass 1: recursive-descent parser ────────────────────────────────────
    // This parser correctly understands string boundaries, so it will ONLY
    // flag real syntax errors (comments outside strings, unquoted keys, trailing
    // commas, etc.) and NOT false-positives inside base64 data URLs or hashes.
    const parserResult = findJsonErrorLocation(content);
    if (parserResult.message !== 'Valid JSON') {
        return [{
            message: parserResult.message,
            line: parserResult.line,
            column: parserResult.col,
        }];
    }

    // ── Pass 2: JSON.parse for anything the hand-written parser missed ───────
    // (e.g. certain edge-cases in numbers or escape sequences)
    try {
        JSON.parse(content);
    } catch (e: any) {
        // Extract line number from browser's JSON.parse error message.
        // Modern engines include "at line X column Y" or "at position X".
        const msg: string = e?.message ?? String(e);
        const lineMatch = /line (\d+)/i.exec(msg);
        const posMatch = /position (\d+)/i.exec(msg);
        let errorLine = 1;
        if (lineMatch) {
            errorLine = parseInt(lineMatch[1], 10);
        } else if (posMatch) {
            // Convert character position → line number
            const pos = parseInt(posMatch[1], 10);
            errorLine = content.substring(0, pos).split('\n').length;
        }
        return [{
            message: msg.replace(/^JSON\.parse:\s*/i, '').replace(/\s*at position \d+/, ''),
            line: errorLine,
        }];
    }

    return [];
}

// ─── TreeNodeView ─────────────────────────────────────────────────────────────

interface TreeNodeViewProps {
    node: TreeNode;
    depth: number;
    collapsedFolders: Set<string>;
    toggleFolder: (key: string) => void;
    selectedFile: ConfigFileInfo | null;
    isDirty: boolean;
    handleSelectFile: (f: ConfigFileInfo) => void;
    profileId: string;
    search: string;
}

function TreeNodeView({
    node,
    depth,
    collapsedFolders,
    toggleFolder,
    selectedFile,
    isDirty,
    handleSelectFile,
    profileId,
    search,
}: TreeNodeViewProps) {
    const isCollapsed = search.trim() !== '' ? false : collapsedFolders.has(node.key);
    const indentPx = depth * 10;
    const fileIndentPx = indentPx + 36;
    const totalFiles = countFiles(node);

    const handleFolderContextMenu = async (e: React.MouseEvent) => {
        e.preventDefault();
        e.stopPropagation();
        const relPath = node.key === '__root__' ? '' : node.key;
        const findRoot = (n: TreeNode): string | undefined => {
            if (n.files.length > 0) return n.files[0].root ?? undefined;
            for (const c of n.children) { const r = findRoot(c); if (r !== undefined) return r; }
            return undefined;
        };
        const root = findRoot(node);
        const { MenuItem: MI, Menu: M } = await import('@tauri-apps/api/menu');
        const menuItem = await MI.new({
            text: revealInFileManagerLabel(),
            action: async () => {
                try { await window.ipcRenderer.revealProfileConfigFile(profileId, relPath, root); }
                catch (err) { console.error('Failed to reveal folder in file manager', err); }
            },
        });
        const menu = await M.new({ items: [menuItem] });
        await menu.popup();
    };

    return (
        <div>
            {/* Folder row */}
            <button
                onClick={() => toggleFolder(node.key)}
                onContextMenu={handleFolderContextMenu}
                style={{ paddingLeft: `${indentPx + 8}px` }}
                className="w-full flex items-center gap-1.5 pr-2 py-1.5 hover:bg-gray-700/50 transition-colors"
            >
                <svg
                    className={`h-3 w-3 text-gray-500 flex-shrink-0 transition-transform duration-150 ${isCollapsed ? '-rotate-90' : ''}`}
                    fill="none" viewBox="0 0 24 24" stroke="currentColor"
                >
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M19 9l-7 7-7-7" />
                </svg>

                {node.iconUrl ? (
                    <img
                        src={node.iconUrl}
                        alt={node.label}
                        className="h-4 w-4 rounded-sm object-cover flex-shrink-0"
                        onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = 'none'; }}
                    />
                ) : (
                    <svg
                        className={`h-3.5 w-3.5 flex-shrink-0 ${depth === 0 ? 'text-blue-400/70' : 'text-gray-500/60'}`}
                        fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}
                    >
                        <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                    </svg>
                )}

                <span className={`text-[11px] truncate flex-1 text-left ${
                    node.isModRoot || depth === 0 ? 'font-semibold text-gray-200' : 'font-medium text-gray-400'
                }`}>
                    {node.label}
                </span>
                <span className="text-[10px] text-gray-600 flex-shrink-0">{totalFiles}</span>
            </button>

            {!isCollapsed && (
                <div>
                    {node.children.map((child) => (
                        <TreeNodeView
                            key={child.key}
                            node={child}
                            depth={depth + 1}
                            collapsedFolders={collapsedFolders}
                            toggleFolder={toggleFolder}
                            selectedFile={selectedFile}
                            isDirty={isDirty}
                            handleSelectFile={handleSelectFile}
                            profileId={profileId}
                            search={search}
                        />
                    ))}

                    {node.files.map((f) => (
                        <button
                            key={f.relative_path}
                            onClick={() => handleSelectFile(f)}
                            onContextMenu={async (e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                const { MenuItem: MI, Menu: M } = await import('@tauri-apps/api/menu');
                                const menuItem = await MI.new({
                                    text: revealInFileManagerLabel(),
                                    action: async () => {
                                        try { await window.ipcRenderer.revealProfileConfigFile(profileId, f.relative_path, f.root ?? undefined); }
                                        catch (err) { console.error('Failed to reveal file in file manager', err); }
                                    },
                                });
                                const menu = await M.new({ items: [menuItem] });
                                await menu.popup();
                            }}
                            style={{ paddingLeft: `${fileIndentPx}px` }}
                            className={`w-full text-left pr-3 py-1.5 flex items-center gap-2 transition-colors ${
                                selectedFile?.relative_path === f.relative_path
                                    ? 'bg-blue-600/20 border-l-2 border-blue-500'
                                    : 'border-l-2 border-transparent hover:bg-gray-700/40'
                            }`}
                        >
                            <span className={`flex-shrink-0 ${selectedFile?.relative_path === f.relative_path ? 'text-blue-400' : 'text-gray-500'}`}>
                                <FileExtIcon name={f.name} className="h-3.5 w-3.5" />
                            </span>
                            <span className={`text-[11px] truncate ${
                                selectedFile?.relative_path === f.relative_path ? 'text-blue-200 font-medium' : 'text-gray-300'
                            }`}>
                                {f.name}
                                {selectedFile?.relative_path === f.relative_path && isDirty && (
                                    <span className="text-amber-400 ml-1">&#9679;</span>
                                )}
                            </span>
                        </button>
                    ))}
                </div>
            )}
        </div>
    );
}

// ─── Main ConfigEditorTab ─────────────────────────────────────────────────────

interface ConfigEditorTabProps {
    profileId: string;
    gameIdentifier?: string | null;
    platform?: string | null;
    mods?: Array<{ fullName: string; iconUrl?: string; displayName?: string }>;
}

export function ConfigEditorTab({ profileId, gameIdentifier, platform, mods = [] }: ConfigEditorTabProps) {
    const { streamMode, username } = useAppStore();
    const [files, setFiles] = useState<ConfigFileInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [selectedFile, setSelectedFile] = useState<ConfigFileInfo | null>(null);

    // Raw content (always the source of truth for saving)
    const [rawContent, setRawContent] = useState('');
    const isLargeFile = rawContent.length > 15728640;
    const isHeavyFile = rawContent.length > 100000;
    // Parsed cfg (only when isCfgFile)
    const [parsedCfg, setParsedCfg] = useState<ParsedCfg | null>(null);
    const [fileLoading, setFileLoading] = useState(false);

    const [isDirty, setIsDirty] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [saveStatus, setSaveStatus] = useState<'idle' | 'saved' | 'error'>('idle');
    const saveStatusTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    // Track which folder nodes are collapsed (key = folder key)
    const [collapsedFolders, setCollapsedFolders] = useState<Set<string>>(new Set());
    const [lineCountState, setLineCountState] = useState(1);

    // Helper: compute line count from raw content without triggering an effect.
    // Threshold mirrors the isHeavyFile constant (100 000 chars).
    const countLines = (content: string) =>
        content.length > 100000 ? 1 : content.split('\n').length || 1;

    // Refs for raw editor line numbers gutter sync
    const gutterRef = useRef<HTMLDivElement>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const searchInputRef = useRef<HTMLInputElement>(null);
    const historyRef = useRef<{ undo: string[]; redo: string[] }>({ undo: [], redo: [] });

    const triggerUndo = useCallback(() => {
        if (parsedCfg) {
            const undoStack = historyRef.current.undo;
            if (undoStack.length > 0) {
                const previousContent = undoStack.pop()!;
                historyRef.current.redo.push(rawContent);
                setRawContent(previousContent);
                setLineCountState(countLines(previousContent));
                try {
                    setParsedCfg(parseCfg(previousContent));
                } catch (e) {
                    console.error('Failed to parse config on undo', e);
                }
                setIsDirty(true);
            }
        } else {
            textareaRef.current?.focus();
            document.execCommand('undo', false);
        }
    }, [parsedCfg, rawContent]);

    const triggerRedo = useCallback(() => {
        if (parsedCfg) {
            const redoStack = historyRef.current.redo;
            if (redoStack.length > 0) {
                const nextContent = redoStack.pop()!;
                historyRef.current.undo.push(rawContent);
                setRawContent(nextContent);
                setLineCountState(countLines(nextContent));
                try {
                    setParsedCfg(parseCfg(nextContent));
                } catch (e) {
                    console.error('Failed to parse config on redo', e);
                }
                setIsDirty(true);
            }
        } else {
            textareaRef.current?.focus();
            document.execCommand('redo', false);
        }
    }, [parsedCfg, rawContent]);

    const handleScroll = useCallback(() => {
        if (textareaRef.current && gutterRef.current) {
            gutterRef.current.scrollTop = textareaRef.current.scrollTop;
        }
    }, []);

    // Keep scroll aligned when content updates
    useEffect(() => {
        if (textareaRef.current && gutterRef.current) {
            gutterRef.current.scrollTop = textareaRef.current.scrollTop;
        }
    }, [rawContent]);

    // Collapsed sections state
    const [collapsedSections, setCollapsedSections] = useState<Set<string>>(new Set());

    // Load file list on mount
    useEffect(() => {
        let cancelled = false;
        const loadFiles = async () => {
            setLoading(true);
            try {
                const result = await window.ipcRenderer.listProfileConfigFiles(profileId, gameIdentifier ?? undefined, platform ?? undefined);
                if (!cancelled) {
                    setFiles(result);
                }
            } catch (e) {
                console.error('Failed to list config files', e);
            } finally {
                if (!cancelled) setLoading(false);
            }
        };
        void loadFiles();
        return () => { cancelled = true; };
    }, [profileId]);

    // Auto-format JSON content helper: pretty-prints valid JSON for display (never marks file dirty)
    const tryPrettyPrintJson = useCallback((raw: string, fileName: string): string => {
        if (!fileName.toLowerCase().endsWith('.json')) return raw;
        if (raw.length > 15728640) return raw; // never touch large files
        // Only reformat if the content looks like a single-line or minified JSON
        const trimmed = raw.trim();
        // If there are very few newlines relative to length, it's likely minified
        const lineCount = (raw.match(/\n/g) || []).length;
        const isMinified = lineCount <= 2 && trimmed.length > 80;
        if (!isMinified) return raw;
        try {
            const parsed = JSON.parse(trimmed);
            return JSON.stringify(parsed, null, 2);
        } catch {
            return raw;
        }
    }, []);

    // Load selected file content
    const loadFile = useCallback(async (file: ConfigFileInfo) => {
        setFileLoading(true);
        setIsDirty(false);
        setSaveStatus('idle');
        setCollapsedSections(new Set());
        historyRef.current = { undo: [], redo: [] };
        try {
            const raw = await window.ipcRenderer.readProfileConfigFile(profileId, file.relative_path, file.root ?? undefined);
            // Auto-format for display only — never marks dirty (user must type to dirty the file)
            const content = tryPrettyPrintJson(raw, file.name);
            setRawContent(content);
            setLineCountState(countLines(content));
            if (textareaRef.current) {
                textareaRef.current.value = content;
            }
            if (isCfgFile(file.name)) {
                setParsedCfg(parseCfg(content));
            } else {
                setParsedCfg(null);
            }
            setSelectedFile(file);
            // isDirty stays false — auto-formatting is display-only
        } catch (e) {
            console.error('Failed to read config file', e);
            setRawContent('');
            setLineCountState(1);
            if (textareaRef.current) {
                textareaRef.current.value = '';
            }
            setParsedCfg(null);
        } finally {
            setFileLoading(false);
        }
    }, [profileId, tryPrettyPrintJson]);

    const handleSelectFile = useCallback((file: ConfigFileInfo) => {
        if (isDirty) {
            // Discard changes and load new file
            // TODO: could show a confirmation dialog
        }
        void loadFile(file);
    }, [loadFile, isDirty]);

    // Update raw content when structured editor changes
    const handleEntryChange = useCallback((sectionIdx: number, entryIdx: number, newValue: string) => {
        // Save current raw content to undo stack before updating
        const currentContent = rawContent;
        const undoStack = historyRef.current.undo;
        if (undoStack.length === 0 || undoStack[undoStack.length - 1] !== currentContent) {
            undoStack.push(currentContent);
            historyRef.current.redo = []; // Clear redo stack on new change
        }

        setParsedCfg((prev) => {
            if (!prev) return prev;
            const next: ParsedCfg = {
                sections: prev.sections.map((s, si) =>
                    si !== sectionIdx ? s : {
                        ...s,
                        entries: s.entries.map((e, ei) =>
                            ei !== entryIdx ? e : { ...e, value: newValue }
                        ),
                    }
                ),
            };
            const serialized = serializeCfg(next);
            setRawContent(serialized);
            setLineCountState(countLines(serialized));
            setIsDirty(true);
            return next;
        });
    }, [rawContent]);

    const handleTextareaInput = useCallback((e: React.FormEvent<HTMLTextAreaElement>) => {
        if (!isDirty) {
            setIsDirty(true);
        }
        if (!isHeavyFile) {
            const val = e.currentTarget.value;
            const lines = val.split('\n').length || 1;
            if (lines !== lineCountState) {
                setLineCountState(lines);
            }
        }
    }, [isDirty, isHeavyFile, lineCountState]);


    const handleSave = useCallback(async () => {
        if (!selectedFile || !isDirty) return;
        const currentContent = (!parsedCfg && textareaRef.current)
            ? (streamMode ? uncensorPath(textareaRef.current.value, rawContent) : textareaRef.current.value)
            : rawContent;

        setIsSaving(true);
        try {
            await window.ipcRenderer.writeProfileConfigFile(profileId, selectedFile.relative_path, currentContent, selectedFile.root ?? undefined);
            setRawContent(currentContent);
            setLineCountState(countLines(currentContent));
            setIsDirty(false);
            setSaveStatus('saved');
        } catch (e) {
            console.error('Failed to save config file', e);
            setSaveStatus('error');
        } finally {
            setIsSaving(false);
            if (saveStatusTimerRef.current) clearTimeout(saveStatusTimerRef.current);
            saveStatusTimerRef.current = setTimeout(() => setSaveStatus('idle'), 3000);
        }
    }, [selectedFile, isDirty, rawContent, parsedCfg, profileId, streamMode, username]);

    // ── Global keyboard shortcuts (work for ALL file types, not just textarea) ──
    useEffect(() => {
        const onKeyDown = (e: KeyboardEvent) => {
            const isMac = e.metaKey;
            const isCtrl = e.ctrlKey;
            if (!isMac && !isCtrl) return;

            const activeTag = document.activeElement?.tagName.toLowerCase();
            const isInputFocused = activeTag === 'input' || activeTag === 'textarea' || (document.activeElement instanceof HTMLElement && document.activeElement.isContentEditable);

            // Cmd/Ctrl + S → Save
            if (e.key === 's' || e.key === 'S') {
                e.preventDefault();
                void handleSave();
                return;
            }

            // Cmd/Ctrl + Z → Undo
            if ((e.key === 'z' || e.key === 'Z') && !e.shiftKey) {
                if (isInputFocused) return; // Let native browser undo handle text inputs
                
                if (parsedCfg) {
                    e.preventDefault();
                    triggerUndo();
                } else if (document.activeElement !== textareaRef.current) {
                    e.preventDefault();
                    triggerUndo();
                }
                return;
            }

            // Cmd/Ctrl + Shift + Z → Redo
            if ((e.key === 'z' || e.key === 'Z') && e.shiftKey) {
                if (isInputFocused) return; // Let native browser redo handle text inputs
                
                if (parsedCfg) {
                    e.preventDefault();
                    triggerRedo();
                } else if (document.activeElement !== textareaRef.current) {
                    e.preventDefault();
                    triggerRedo();
                }
                return;
            }
        };
        document.addEventListener('keydown', onKeyDown);
        return () => document.removeEventListener('keydown', onKeyDown);
    }, [handleSave, triggerUndo, triggerRedo, parsedCfg]);

    const handleDiscard = useCallback(() => {
        if (!selectedFile) return;
        void loadFile(selectedFile);
    }, [selectedFile, loadFile]);

    const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
        if (isHeavyFile) {
            // Bypass extra event logic on large files for raw performance.
            // Only capture Cmd+S / Ctrl+S to save.
            if ((e.metaKey || e.ctrlKey) && e.key === 's') {
                e.preventDefault();
                void handleSave();
            }
            return;
        }

        const textarea = e.currentTarget;
        const { selectionStart, selectionEnd, value } = textarea;

        // A. Cmd+S / Ctrl+S (Save)
        if ((e.metaKey || e.ctrlKey) && e.key === 's') {
            e.preventDefault();
            void handleSave();
            return;
        }


        // C. Cmd+/ / Ctrl+/ (Toggle Comment)
        if ((e.metaKey || e.ctrlKey) && e.key === '/') {
            e.preventDefault();
            const start = textarea.selectionStart;
            const end = textarea.selectionEnd;
            const before = value.substring(0, start);
            const after = value.substring(end);
            
            const lineStartPos = before.lastIndexOf('\n') + 1;
            let lineEndPos = end + after.indexOf('\n');
            if (after.indexOf('\n') === -1) {
                lineEndPos = value.length;
            }
            
            const allLinesText = value.substring(lineStartPos, lineEndPos);
            const lines = allLinesText.split('\n');
            
            const isJson = selectedFile?.name.endsWith('.json') || selectedFile?.name.endsWith('.txt');
            const commentSymbol = isJson ? '//' : '#';
            
            const allCommented = lines.every(line => line.trim().startsWith(commentSymbol));
            
            const newLines = lines.map(line => {
                if (allCommented) {
                    const trimStart = line.trimStart();
                    if (trimStart.startsWith(commentSymbol)) {
                        const commentIndex = line.indexOf(commentSymbol);
                        return line.substring(0, commentIndex) + line.substring(commentIndex + commentSymbol.length).replace(/^\s/, '');
                    }
                    return line;
                } else {
                    const match = line.match(/^(\s*)/);
                    const spaces = match ? match[0] : '';
                    return spaces + commentSymbol + ' ' + line.substring(spaces.length);
                }
            });
            
            const replacement = newLines.join('\n');
            textarea.selectionStart = lineStartPos;
            textarea.selectionEnd = lineEndPos;
            
            document.execCommand('insertText', false, replacement);
            
            textarea.selectionStart = lineStartPos;
            textarea.selectionEnd = lineStartPos + replacement.length;
            return;
        }

        // 1. Tab support
        if (e.key === 'Tab') {
            e.preventDefault();
            document.execCommand('insertText', false, '    ');
            return;
        }

        // 2. Auto-closing pairs
        const pairs: Record<string, string> = {
            '{': '}',
            '[': ']',
            '(': ')',
            '"': '"',
            "'": "'",
        };

        if (pairs[e.key] !== undefined) {
            e.preventDefault();
            const closePair = pairs[e.key];
            const selectionText = value.substring(selectionStart, selectionEnd);
            document.execCommand('insertText', false, e.key + selectionText + closePair);
            
            if (selectionStart === selectionEnd) {
                textarea.selectionStart = textarea.selectionEnd = selectionStart + 1;
            } else {
                textarea.selectionStart = selectionStart + 1;
                textarea.selectionEnd = selectionEnd + 1;
            }
            return;
        }

        // 3. Close character bypass
        const closingCharacters = ['}', ']', ')', '"', "'"];
        if (closingCharacters.includes(e.key) && selectionStart === selectionEnd) {
            const nextChar = value.charAt(selectionStart);
            if (nextChar === e.key) {
                e.preventDefault();
                textarea.selectionStart = textarea.selectionEnd = selectionStart + 1;
                return;
            }
        }

        // 4. Backspace helper
        if (e.key === 'Backspace' && selectionStart === selectionEnd && selectionStart > 0) {
            const prevChar = value.charAt(selectionStart - 1);
            const nextChar = value.charAt(selectionStart);
            const backspacePairs: Record<string, string> = {
                '{': '}',
                '[': ']',
                '(': ')',
                '"': '"',
                "'": "'",
            };
            if (backspacePairs[prevChar] === nextChar) {
                e.preventDefault();
                textarea.selectionStart = selectionStart - 1;
                textarea.selectionEnd = selectionStart + 1;
                document.execCommand('delete', false);
                return;
            }
        }

        // 5. Smart indentation on Enter
        if (e.key === 'Enter' && selectionStart === selectionEnd) {
            const beforeCursor = value.substring(0, selectionStart);
            const lines = beforeCursor.split('\n');
            const currentLine = lines[lines.length - 1];
            
            const indentMatch = currentLine.match(/^(\s*)/);
            const currentIndent = indentMatch ? indentMatch[1] : '';
            
            const prevChar = value.charAt(selectionStart - 1);
            const nextChar = value.charAt(selectionStart);
            
            if ((prevChar === '{' && nextChar === '}') || (prevChar === '[' && nextChar === ']')) {
                e.preventDefault();
                const extraIndent = '    ';
                const insertText = '\n' + currentIndent + extraIndent + '\n' + currentIndent;
                document.execCommand('insertText', false, insertText);
                
                const cursorPosition = selectionStart + 1 + currentIndent.length + extraIndent.length;
                textarea.selectionStart = textarea.selectionEnd = cursorPosition;
                return;
            } else if (prevChar === '{' || prevChar === '[' || prevChar === ':') {
                e.preventDefault();
                const extraIndent = '    ';
                const insertText = '\n' + currentIndent + extraIndent;
                document.execCommand('insertText', false, insertText);
                return;
            } else if (currentIndent.length > 0) {
                e.preventDefault();
                const insertText = '\n' + currentIndent;
                document.execCommand('insertText', false, insertText);
                return;
            }
        }
    }, [handleSave, selectedFile, isHeavyFile]);

    const toggleSection = useCallback((name: string) => {
        setCollapsedSections((prev) => {
            const next = new Set(prev);
            if (next.has(name)) next.delete(name);
            else next.add(name);
            return next;
        });
    }, []);

    // JSON lint state
    const isJsonFile = selectedFile?.name.toLowerCase().endsWith('.json') ?? false;

    const jsonErrors = useMemo(() => {
        if (!isJsonFile || !rawContent || isHeavyFile) return [];
        return lintJSONAll(rawContent);
    }, [isJsonFile, rawContent, isHeavyFile]);

    const errorLineNumbers = useMemo(() => {
        return new Set(jsonErrors.map((e) => e.line));
    }, [jsonErrors]);

    const folderTree = useMemo(
        () => buildTree(files, mods, search),
        [files, mods, search]
    );

    const toggleFolder = useCallback((key: string) => {
        setCollapsedFolders((prev) => {
            const next = new Set(prev);
            if (next.has(key)) next.delete(key);
            else next.add(key);
            return next;
        });
    }, []);

    // ── Render ──────────────────────────────────────────────────────────────

    if (loading) {
        return (
            <div className="flex items-center justify-center py-16">
                <svg className="animate-spin h-8 w-8 text-blue-500" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
            </div>
        );
    }

    if (files.length === 0) {
        return (
            <div className="flex flex-col items-center justify-center py-16 text-center px-4">
                <div className="w-16 h-16 mb-4 rounded-xl bg-gray-800 border border-gray-700 flex items-center justify-center opacity-50">
                    <svg className="h-8 w-8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                        <circle cx="12" cy="12" r="3" />
                        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                    </svg>
                </div>
                <p className="text-gray-400 font-medium mb-1">No config files found</p>
                <p className="text-gray-600 text-sm max-w-xs">
                    Make sure the game directory is set in <strong className="text-gray-500">Settings → Game Directory</strong>, then apply your profile to the game to generate BepInEx config files.
                </p>
            </div>
        );
    }

    return (
        <div className="flex gap-0 h-[520px] min-h-0">
        {/* ── File Explorer Panel ──────────────────────────────────────────── */}
            <div className="w-60 flex-shrink-0 border-r border-gray-700/80 flex flex-col bg-[#111827]/60">
                {/* Search */}
                <div className="p-2 border-b border-gray-700/80 flex-shrink-0">
                    <div className="relative">
                        <svg className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-gray-500 pointer-events-none" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                        </svg>
                        <input
                            ref={searchInputRef}
                            type="text"
                            value={search}
                            onChange={(e) => setSearch(e.target.value)}
                            placeholder="Search files…"
                            className="w-full pl-8 pr-2 py-1.5 bg-gray-800/70 border border-gray-700/70 rounded-md text-xs text-white placeholder-gray-500 focus:outline-none focus:border-blue-500/70 focus:bg-gray-800"
                        />
                    </div>
                </div>

                {/* Folder tree */}
                <div className="flex-1 overflow-y-auto py-1">
                    {folderTree.length === 0 ? (
                        <p className="text-[11px] text-gray-500 text-center py-6 px-3">
                            {search ? 'No matches' : 'No config files found'}
                        </p>
                    ) : (
                        folderTree.map((node) => (
                            <TreeNodeView
                                key={node.key}
                                node={node}
                                depth={0}
                                collapsedFolders={collapsedFolders}
                                toggleFolder={toggleFolder}
                                selectedFile={selectedFile}
                                isDirty={isDirty}
                                handleSelectFile={handleSelectFile}
                                profileId={profileId}
                                search={search}
                            />
                        ))
                    )}
                </div>
            </div>

            {/* ── Editor Panel ─────────────────────────────────────────────────── */}
            <div className="flex-1 flex flex-col min-w-0">
                {!selectedFile ? (
                    <div className="flex flex-col items-center justify-center h-full gap-3">
                        <div className="opacity-20">
                            <svg className="h-12 w-12 mx-auto" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                                <circle cx="12" cy="12" r="3" />
                                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                            </svg>
                        </div>
                        <p className="text-gray-500 text-sm">Select a file to edit</p>
                        <p className="text-gray-600 text-xs">⌘S to save &nbsp;·&nbsp; ⌘Z undo &nbsp;·&nbsp; ⌘⇧Z redo</p>
                    </div>
                ) : fileLoading ? (
                    <div className="flex items-center justify-center h-full">
                        <svg className="animate-spin h-6 w-6 text-blue-500" fill="none" viewBox="0 0 24 24">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                        </svg>
                    </div>
                ) : (
                    <>
                        {/* Editor Header */}
                        <div className="flex items-center justify-between px-4 py-2.5 border-b border-gray-700 flex-shrink-0">
                            <div className="min-w-0">
                                <p className="text-sm font-semibold text-white truncate">
                                    {selectedFile.name}
                                    {isDirty && <span className="text-amber-400 ml-1 text-xs">● unsaved</span>}
                                </p>
                                <p className="text-[10px] text-gray-500 truncate">{selectedFile.relative_path}</p>
                            </div>
                            <div className="flex items-center gap-2 flex-shrink-0 ml-3">
                                {saveStatus === 'saved' && (
                                    <span className="text-xs text-green-400 flex items-center gap-1">
                                        <svg className="h-3.5 w-3.5" fill="currentColor" viewBox="0 0 20 20">
                                            <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                                        </svg>
                                        Saved
                                    </span>
                                )}
                                {saveStatus === 'error' && (
                                    <span className="text-xs text-red-400">Save failed</span>
                                )}
                                {!isLargeFile && !parsedCfg && (
                                    <>
                                        <button
                                            onClick={triggerUndo}
                                            title="Undo (Cmd+Z)"
                                            className="text-xs text-gray-400 hover:text-white p-1.5 rounded-lg hover:bg-gray-700 transition-colors flex items-center justify-center"
                                        >
                                            <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a8 8 0 018 8v2M3 10l6 6m-6-6l6-6" />
                                            </svg>
                                        </button>
                                        <button
                                            onClick={triggerRedo}
                                            title="Redo (Cmd+Shift+Z)"
                                            className="text-xs text-gray-400 hover:text-white p-1.5 rounded-lg hover:bg-gray-700 transition-colors flex items-center justify-center"
                                        >
                                            <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 10H11a8 8 0 00-8 8v2M21 10l-6 6m6-6l-6-6" />
                                            </svg>
                                        </button>
                                        {isJsonFile && (
                                            <button
                                                onClick={() => {
                                                    const current = textareaRef.current?.value ?? rawContent;
                                                    try {
                                                        const pretty = JSON.stringify(JSON.parse(current), null, 2);
                                                        setRawContent(pretty);
                                                        setLineCountState(countLines(pretty));
                                                        if (textareaRef.current) textareaRef.current.value = pretty;
                                                        setIsDirty(true);
                                                    } catch { /* ignore if invalid JSON */ }
                                                }}
                                                title="Format JSON"
                                                className="text-xs text-gray-400 hover:text-white p-1.5 rounded-lg hover:bg-gray-700 transition-colors flex items-center justify-center"
                                            >
                                                <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                                                    <path strokeLinecap="round" strokeLinejoin="round" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                                                </svg>
                                            </button>
                                        )}
                                        <div className="h-4 w-px bg-gray-700 mx-1" />
                                    </>
                                )}
                                <button
                                    onClick={async () => {
                                        try {
                                            await window.ipcRenderer.revealProfileConfigFile(
                                                profileId,
                                                selectedFile.relative_path,
                                                selectedFile.root ?? undefined
                                            );
                                        } catch (err) {
                                            console.error('Failed to reveal file in file manager', err);
                                        }
                                    }}
                                    title={revealInFileManagerLabel()}
                                    className="text-xs text-gray-400 hover:text-white p-1.5 rounded-lg hover:bg-gray-700 transition-colors flex items-center justify-center"
                                >
                                    <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                                        <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
                                    </svg>
                                </button>
                                {!isLargeFile && isDirty && (
                                    <button
                                        onClick={handleDiscard}
                                        className="text-xs text-gray-400 hover:text-white px-2 py-1 rounded-lg hover:bg-gray-700 transition-colors"
                                    >
                                        Discard
                                    </button>
                                )}
                                {!isLargeFile && (
                                    <button
                                        onClick={() => void handleSave()}
                                        disabled={!isDirty || isSaving}
                                        className={`text-xs font-medium px-3 py-1.5 rounded-lg transition-colors flex items-center gap-1.5 ${
                                            isDirty && !isSaving
                                                ? 'bg-blue-600 hover:bg-blue-500 text-white'
                                                : 'bg-gray-700 text-gray-500 cursor-not-allowed'
                                        }`}
                                    >
                                        {isSaving ? (
                                            <>
                                                <svg className="animate-spin h-3 w-3" fill="none" viewBox="0 0 24 24">
                                                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                                                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                                                </svg>
                                                Saving…
                                            </>
                                        ) : (
                                            'Save'
                                        )}
                                    </button>
                                )}
                            </div>
                        </div>

                        {/* Editor Body */}
                        <div className="flex-1 overflow-y-auto">
                            {isLargeFile ? (
                                <div className="flex flex-col items-center justify-center h-full px-6 text-center max-w-sm mx-auto">
                                    <div className="p-3 bg-amber-500/10 text-amber-500 rounded-2xl mb-4 border border-amber-500/20">
                                        <svg className="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                                            <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                        </svg>
                                    </div>
                                    <h3 className="text-sm font-bold text-gray-200 mb-1">
                                        File too large to edit inside the app
                                    </h3>
                                    <p className="text-[11px] text-gray-500 leading-relaxed mb-6">
                                        This file is <span className="text-amber-400 font-semibold">{Math.round(rawContent.length / 1024)} KB</span>. Editing large files inside the manager can freeze or crash the program.
                                    </p>
                                    <div className="flex flex-col gap-2 w-full">
                                        <button
                                            onClick={async () => {
                                                try {
                                                    await window.ipcRenderer.openProfileConfigFile(
                                                        profileId,
                                                        selectedFile.relative_path,
                                                        selectedFile.root ?? undefined
                                                    );
                                                } catch (err) {
                                                    console.error('Failed to open file externally', err);
                                                }
                                            }}
                                            className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-xs font-semibold transition-colors"
                                        >
                                            <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                                                <path strokeLinecap="round" strokeLinejoin="round" d="M13.5 6H5.25A2.25 2.25 0 003 8.25v10.5A2.25 2.25 0 005.25 21h10.5A2.25 2.25 0 0018 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" />
                                            </svg>
                                            Open with External Editor
                                        </button>
                                        <button
                                            onClick={async () => {
                                                try {
                                                    await window.ipcRenderer.revealProfileConfigFile(
                                                        profileId,
                                                        selectedFile.relative_path,
                                                        selectedFile.root ?? undefined
                                                    );
                                                } catch (err) {
                                                    console.error('Failed to reveal file in Finder', err);
                                                }
                                            }}
                                            className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-gray-800 hover:bg-gray-750 text-gray-300 rounded-lg text-xs font-semibold border border-gray-700/80 transition-colors"
                                        >
                                            <svg className="h-3.5 w-3.5 text-blue-400/80" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                                                <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
                                            </svg>
                                            {revealInFileManagerLabel()}
                                        </button>
                                    </div>
                                </div>
                            ) : parsedCfg ? (
                                /* ── Structured .cfg editor ────────────────────── */
                                <div className="p-4 space-y-4">
                                    {parsedCfg.sections.length === 0 ? (
                                        <p className="text-gray-500 text-sm text-center py-6">
                                            No structured sections found. Try the raw editor view.
                                        </p>
                                    ) : (
                                        parsedCfg.sections.map((section, si) => (
                                            <div key={section.name} className="rounded-xl border border-gray-700 overflow-hidden">
                                                {/* Section Header */}
                                                <button
                                                    className="w-full flex items-center justify-between px-4 py-2.5 bg-gray-800 hover:bg-gray-750 transition-colors text-left"
                                                    onClick={() => toggleSection(section.name)}
                                                >
                                                    <span className="text-sm font-bold text-gray-200">
                                                        {section.name}
                                                    </span>
                                                    <span className="flex items-center gap-2">
                                                        <span className="text-xs text-gray-500">
                                                            {section.entries.length} {section.entries.length === 1 ? 'setting' : 'settings'}
                                                        </span>
                                                        <svg
                                                            className={`h-4 w-4 text-gray-500 transition-transform ${
                                                                collapsedSections.has(section.name) ? '-rotate-90' : ''
                                                            }`}
                                                            fill="none"
                                                            stroke="currentColor"
                                                            viewBox="0 0 24 24"
                                                        >
                                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                                                        </svg>
                                                    </span>
                                                </button>

                                                {/* Section Entries */}
                                                {!collapsedSections.has(section.name) && (
                                                    <div className="px-4 bg-gray-900">
                                                        {section.entries.length === 0 ? (
                                                            <p className="text-xs text-gray-600 py-3">No entries in this section</p>
                                                        ) : (
                                                            section.entries.map((entry, ei) => (
                                                                <EntryEditor
                                                                    key={entry.key}
                                                                    entry={entry}
                                                                    onChange={(v) => handleEntryChange(si, ei, v)}
                                                                />
                                                            ))
                                                        )}
                                                    </div>
                                                )}
                                            </div>
                                        ))
                                    )}
                                </div>
                            ) : (
                                /* ── Raw text editor ───────────────────────────── */
                                <div className="h-full flex flex-col min-h-0 bg-gray-950 border border-gray-700 overflow-hidden">
                                    {/* Editor Workspace: Gutter + Textarea */}
                                    <div className="flex-1 flex min-h-0 relative">
                                        {/* Line Numbers Gutter */}
                                        {/* Line Numbers Gutter */}
                                        {!isHeavyFile && (
                                            <div
                                                ref={gutterRef}
                                                className="w-10 bg-gray-900/30 border-r border-gray-800/80 text-right pr-2.5 py-2 font-mono text-[11px] text-gray-500 select-none overflow-hidden h-full flex flex-col"
                                            >
                                                {Array.from({ length: lineCountState }).map((_, i) => {
                                                    const lineNum = i + 1;
                                                    const isErrorLine = errorLineNumbers.has(lineNum);
                                                    return (
                                                        <div
                                                            key={lineNum}
                                                            style={{ height: '20px', lineHeight: '20px' }}
                                                            className={`pr-1 transition-colors duration-150 ${
                                                                isErrorLine
                                                                    ? 'text-red-400 font-bold bg-red-950/40 border-r-2 border-red-500'
                                                                    : 'hover:text-gray-400'
                                                            }`}
                                                        >
                                                            {lineNum}
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                        )}

                                        {/* Textarea */}
                                        <textarea
                                            key={`${selectedFile.relative_path}_${streamMode}`}
                                            ref={textareaRef}
                                            defaultValue={streamMode ? censorPath(rawContent, username) : rawContent}
                                            onInput={handleTextareaInput}
                                            onKeyDown={handleKeyDown}
                                            onScroll={handleScroll}
                                            spellCheck={false}
                                            autoCorrect="off"
                                            autoCapitalize="off"
                                            wrap="off"
                                            style={{ lineHeight: '20px' }}
                                            className="flex-1 resize-none bg-transparent px-3 py-2 text-[11px] font-mono text-gray-200 focus:outline-none h-full overflow-auto whitespace-pre border-0 ring-0 outline-none"
                                            placeholder="Empty file"
                                        />
                                    </div>

                                    {/* Error Banner at the Bottom */}
                                    {isJsonFile && jsonErrors.length > 0 && (
                                        <div className="flex-shrink-0 px-4 py-3 bg-red-950/35 border-t border-red-500/25 text-red-200 text-xs flex flex-col gap-2 max-h-[120px] overflow-y-auto">
                                            <div className="flex items-center gap-2 font-semibold text-red-300">
                                                <svg className="h-4 w-4 text-red-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                                                </svg>
                                                <span>JSON Syntax Errors ({jsonErrors.length}):</span>
                                            </div>
                                            <ul className="space-y-1.5 pl-6 list-disc">
                                                {jsonErrors.map((err, idx) => (
                                                    <li key={idx} className="leading-relaxed">
                                                        {err.message}{' '}
                                                        <span className="text-red-400 font-mono font-semibold">
                                                            (Line {err.line}{err.column ? `, Col ${err.column}` : ''})
                                                        </span>
                                                    </li>
                                                ))}
                                            </ul>
                                        </div>
                                    )}
                                </div>
                            )}
                        </div>
                    </>
                )}
            </div>

        </div>
    );
}
