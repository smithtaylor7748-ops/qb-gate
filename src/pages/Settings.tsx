import { useState } from 'react';
import { api } from '../lib/api';
import { useAction } from '../lib/workspace';
import { Button, Modal } from '../ui';
import { ManagedDirControl } from './managed/ManagedPanel';
export default function Settings() {
 const action=useAction();const [restore,setRestore]=useState(false);
 return <><ManagedDirControl/><section className="qb-detail"><h2>恢复系统时区</h2><p className="qb-muted">如果本次运行中通过环境修复调整了时区，可以恢复调整前的记录。</p><Button onClick={()=>setRestore(true)}>恢复时区</Button>{action.error && <p className="notice notice--danger">{action.error}</p>}</section><Modal open={restore} onClose={()=>setRestore(false)} title="恢复系统时区" footer={<Button loading={!!action.pending} onClick={()=>void action.run('timezone',()=>api.tzRestore().then(()=>true),'系统时区已恢复').then(ok=>{if(ok)setRestore(false);})}>确认恢复</Button>}><p>将系统时区恢复为本次调整前的值；其他应用也会看到这个时区变化。</p></Modal></>;
}
