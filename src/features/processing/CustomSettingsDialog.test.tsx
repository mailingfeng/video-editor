import {useState} from 'react';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {afterEach,beforeAll,expect,it,vi} from 'vitest';
import fixture from '../../../tests/fixtures/custom-settings.json';
import type {CustomVariationSettings} from '../../api/contracts';
import {CustomSettingsDialog} from './CustomSettingsDialog';
afterEach(cleanup);
beforeAll(()=>{HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};});
const recommended:CustomVariationSettings={...fixture,schemaVersion:1};
function setup(busy=false,saving=false){
 const cancel=vi.fn(),save=vi.fn(async()=>true),restore=vi.fn();
 function Harness({busy,saving}:{busy:boolean;saving:boolean}){
  const [draft,setDraft]=useState(recommended);
  return <CustomSettingsDialog draft={draft} recommended={recommended} busy={busy} saving={saving} error={null} onLevelChange={(key,value)=>setDraft(s=>({...s,[key]:value}))} onRestore={restore} onCancel={cancel} onSave={save}/>;
 }
 const view=render(<Harness busy={busy} saving={saving}/>);return {...view,cancel,save,restore,Harness};
}
it('all_fourteen_levels_are_accessible',()=>{
 setup();expect(screen.getByRole('dialog',{name:'自定义变化设置'})).toBeInTheDocument();
 expect(screen.getAllByRole('slider')).toHaveLength(14);
 expect(screen.getByRole('slider',{name:'前景缩放'})).toHaveValue('1');expect(screen.getByRole('slider',{name:'背景模糊'})).toHaveValue('1');expect(screen.getByRole('slider',{name:'变速'})).toHaveValue('0');
 for(const slider of screen.getAllByRole('slider')){expect(slider).toHaveAttribute('min','0');expect(slider).toHaveAttribute('max','9');expect(slider).toHaveAttribute('step','1');}
 expect(screen.getByText(/可能裁掉边缘字幕/)).toBeInTheDocument();
});
it('dependencies_preserve_hidden_values',()=>{
 setup();fireEvent.change(screen.getByRole('slider',{name:'背景模糊'}),{target:{value:'7'}});
 fireEvent.change(screen.getByRole('slider',{name:'前景缩放'}),{target:{value:'0'}});
 expect(screen.getByRole('slider',{name:'背景模糊'})).toBeDisabled();expect(screen.getByRole('slider',{name:'动态位移'})).toBeDisabled();
 fireEvent.change(screen.getByRole('slider',{name:'前景缩放'}),{target:{value:'2'}});
 expect(screen.getByRole('slider',{name:'背景模糊'})).toBeEnabled();expect(screen.getByRole('slider',{name:'背景模糊'})).toHaveValue('7');
});
it('external_start_locks_draft',()=>{
 const {rerender,Harness,cancel,save}=setup();fireEvent.change(screen.getByRole('slider',{name:'变速'}),{target:{value:'8'}});
 rerender(<Harness busy saving={false}/>);expect(screen.getByRole('slider',{name:'变速'})).toBeDisabled();expect(screen.getByRole('slider',{name:'变速'})).toHaveValue('8');
 expect(screen.getByRole('button',{name:'保存设置'})).toBeDisabled();fireEvent.click(screen.getByRole('button',{name:'取消'}));expect(cancel).toHaveBeenCalledOnce();expect(save).not.toHaveBeenCalled();
});
it('saving_blocks_escape_and_close',()=>{
 const {cancel}=setup(false,true);fireEvent(screen.getByRole('dialog'),new Event('cancel',{cancelable:true}));
 expect(cancel).not.toHaveBeenCalled();expect(screen.getByRole('button',{name:'取消'})).toBeDisabled();expect(screen.getByRole('button',{name:'关闭'})).toBeDisabled();
});
