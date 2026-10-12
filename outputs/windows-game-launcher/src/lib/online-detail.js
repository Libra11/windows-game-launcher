import qrcode from 'qrcode-generator';

export function resourceQrCode(value) {
  const url = new URL(value);
  if (url.protocol !== 'https:' || url.username || url.password || url.href.length > 2000) throw Error('网盘地址无法生成二维码');
  const qr = qrcode(0, 'M'); qr.addData(url.href); qr.make();
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(qr.createSvgTag({cellSize:4,margin:16}))}`;
}

export function createOnlineDetailController(command, changed) {
  const state = {item:null,data:null,loading:false,error:''};
  let version = 0;
  async function load(item = state.item) {
    if (!item) return;
    const same = state.item?.url === item.url;
    if (same && state.loading) return;
    const current = ++version;
    state.item = item; if (!same) state.data = null;
    state.loading = true; state.error = ''; changed(state);
    try {
      const data = await command('get_gamer520_detail',{url:item.url});
      if (current === version) state.data = data;
    } catch (error) { if (current === version) state.error = String(error); }
    finally { if (current === version) {state.loading = false; changed(state);} }
  }
  return {state,load,cancel(){version++;state.loading=false;}};
}
