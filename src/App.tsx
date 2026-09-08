import { useCallback, useEffect, useState } from 'react';
import { api, type Progress, type Risk, type StepState } from './lib/api';
import { ALWAYS_AVAILABLE, RISK_LABEL, STEPS, type StepId } from './lib/steps';
import Dashboard from './pages/Dashboard';
import Purity from './pages/Purity';
import Environment from './pages/Environment';
import DnsLeak from './pages/DnsLeak';
import IpLock from './pages/IpLock';
import Accounts from './pages/Accounts';
import Relay from './pages/Relay';
import Settings from './pages/Settings';

export type PageId = 'dashboard' | StepId | 'relay' | 'settings';

export interface StepApi {
  progress: Progress;
  /** 记录本步结果，并刷新侧栏状态灯与风险标。 */
  mark: (id: StepId, state: StepState, risk: Risk, detail: string) => Promise<void>;
  go: (page: PageId) => void;
}

const EMPTY: Progress = { steps: {}, completed_once: false };

export default function App() {
  const [page, setPage] = useState<PageId>('dashboard');
  const [progress, setProgress] = useState<Progress>(EMPTY);

  const reload = useCallback(async () => {
    try {
      setProgress(await api.progressLoad());
    } catch {
      /* 进度文件读不出来就用空的，不该因此挡住整个界面 */
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const mark = useCallback(
    async (id: StepId, state: StepState, risk: Risk, detail: string) => {
      try {
        setProgress(await api.progressSet(id, state, risk, detail));
      } catch {
        /* 同上 */
      }
    },
    []
  );

  const stepApi: StepApi = { progress, mark, go: setPage };

  const skipped = Object.values(progress.steps).filter((s) => s.state === 'skipped').length;

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          ClaudeGate
          <small>
            {progress.completed_once
              ? skipped > 0
                ? `已走完一轮 · 跳过 ${skipped} 项`
                : '已走完一轮'
              : `进度 ${Object.keys(progress.steps).length} / ${STEPS.length}`}
          </small>
        </div>

        <div className="navgroup">
          <button
            className={`navitem${page === 'dashboard' ? ' active' : ''}`}
            onClick={() => setPage('dashboard')}
          >
            <span className="stepnum">·</span>
            总览
          </button>
        </div>

        <div className="navgroup">
          <div className="navgroup-title">
            {progress.completed_once ? '检测项' : '按顺序往下走'}
          </div>
          {STEPS.map((s, i) => {
            const rec = progress.steps[s.id];
            const state = rec?.state ?? 'pending';
            const risk = rec?.risk ?? 'unknown';
            return (
              <button
                key={s.id}
                className={`navitem${page === s.id ? ' active' : ''}`}
                onClick={() => setPage(s.id)}
                title={rec?.detail || s.blurb}
              >
                <span className={`stepnum ${state}`}>
                  {/* 走完一轮后序号换成状态灯 */}
                  {progress.completed_once && state !== 'pending'
                    ? state === 'passed'
                      ? '✓'
                      : state === 'skipped'
                        ? '–'
                        : '!'
                    : i + 1}
                </span>
                {s.label}
                <span className={`risk ${risk}`}>{RISK_LABEL[risk]}</span>
              </button>
            );
          })}
        </div>

        <div className="navgroup">
          <div className="navgroup-title">随时可开</div>
          {ALWAYS_AVAILABLE.map((s) => (
            <button
              key={s.id}
              className={`navitem${page === s.id ? ' active' : ''}`}
              onClick={() => setPage(s.id as PageId)}
            >
              <span className="stepnum">·</span>
              {s.label}
            </button>
          ))}
        </div>
      </nav>

      <main className="main">
        <div className="main-inner">
          {page === 'dashboard' && <Dashboard {...stepApi} />}
          {page === 'purity' && <Purity {...stepApi} />}
          {page === 'environment' && <Environment {...stepApi} />}
          {page === 'dns' && <DnsLeak {...stepApi} />}
          {page === 'iplock' && <IpLock {...stepApi} />}
          {page === 'accounts' && <Accounts {...stepApi} />}
          {page === 'relay' && <Relay />}
          {page === 'settings' && <Settings />}
        </div>
      </main>
    </div>
  );
}
