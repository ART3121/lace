// Busca em todos os arquivos de texto do projeto (Ctrl+Shift+F).

import { CaseSensitive, Regex } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { useT } from '../../i18n';
import { api } from '../../ipc/api';
import type { IpcError, SearchMatch } from '../../ipc/types';
import { useEditor } from '../../state/editor';
import { useProject } from '../../state/project';
import { relativeTo } from '../../util/paths';
import { Empty, IconButton, Spinner } from '../common';

const LIMIT = 2000;

export function SearchPanel() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  const [query, setQuery] = useState('');
  const [regex, setRegex] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [results, setResults] = useState<SearchMatch[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => input.current?.focus(), []);

  useEffect(() => {
    if (!snapshot || !query) {
      setResults([]);
      setError(null);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setSearching(true);
      api.fs
        .search(query, regex, caseSensitive, LIMIT)
        .then((found) => {
          if (cancelled) return;
          setResults(found);
          setError(null);
        })
        .catch((e: IpcError) => !cancelled && setError(e.message))
        .finally(() => !cancelled && setSearching(false));
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, regex, caseSensitive, snapshot]);

  const grouped = useMemo(() => {
    const map = new Map<string, SearchMatch[]>();
    for (const match of results) {
      const list = map.get(match.path) ?? [];
      list.push(match);
      map.set(match.path, list);
    }
    return [...map.entries()];
  }, [results]);

  if (!snapshot) {
    return (
      <div className="sidebar__body">
        <Empty>{t('explorer.noProject')}</Empty>
      </div>
    );
  }

  return (
    <div className="sidebar__body search">
      <div className="search__box">
        <input
          ref={input}
          className="input"
          placeholder={t('search.placeholder')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <IconButton label={t('search.caseSensitive')} active={caseSensitive} onClick={() => setCaseSensitive(!caseSensitive)}>
          <CaseSensitive size={15} />
        </IconButton>
        <IconButton label={t('search.regex')} active={regex} onClick={() => setRegex(!regex)}>
          <Regex size={15} />
        </IconButton>
      </div>
      {error && <p className="text-error search__status">{error}</p>}
      {query && !error && (
        <p className="muted search__status">
          {searching ? <Spinner size={12} /> : null}{' '}
          {results.length === 0 && !searching
            ? t('search.noResults')
            : t('search.results', { count: results.length, files: grouped.length })}
          {results.length >= LIMIT && ` ${t('search.limit', { count: LIMIT })}`}
        </p>
      )}
      <div className="search__results">
        {grouped.map(([path, matches]) => (
          <div key={path} className="search__file">
            <div className="search__path" title={path}>
              {relativeTo(path, snapshot.root)} <span className="muted">({matches.length})</span>
            </div>
            {matches.map((m) => (
              <button
                key={`${m.line}:${m.column}`}
                type="button"
                className="search__match"
                onClick={() => void useEditor.getState().openFile(path, { line: m.line, column: m.column })}
              >
                <span className="search__line">{m.line}</span>
                <span className="search__text">
                  {m.text.slice(Math.max(0, m.column - 40), m.column - 1)}
                  <mark>{m.text.slice(m.column - 1, m.column - 1 + m.length)}</mark>
                  {m.text.slice(m.column - 1 + m.length, m.column + 120)}
                </span>
              </button>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
