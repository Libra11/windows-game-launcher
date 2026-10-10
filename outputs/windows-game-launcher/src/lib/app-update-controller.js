export function createAppUpdateController(command,listen) {
  let status,stop,disposed=false,revision=0;const subscribers=new Set();
  function commit(next){if(disposed||status&&next.revision<status.revision)return;revision++;status=next;for(const handler of subscribers)handler(status);}
  async function start(){
    stop=await listen(event=>commit(event.payload));
    const before=revision;const next=await command('get_app_update_status');if(before===revision)commit(next);
  }
  async function perform(name,args){const next=await command(name,args);commit(next);return next;}
  const result={get status(){return status;},subscribe(handler){subscribers.add(handler);if(status)handler(status);return ()=>subscribers.delete(handler);},start,
    check:()=>perform('check_app_update'),download:id=>perform('download_app_update',{preparationId:id}),install:id=>perform('install_app_update',{preparationId:id}),
    cancel:operationId=>command('cancel_app_update_download',{operationId}),refresh:()=>perform('get_app_update_status')};
  if(import.meta.hot)import.meta.hot.dispose(()=>{disposed=true;stop?.();subscribers.clear();});return result;
}
