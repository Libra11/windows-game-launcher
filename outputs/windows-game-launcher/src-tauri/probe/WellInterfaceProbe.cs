using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

// 仅针对已检查的 GDKExtension.dll 版本，使用硬件断点，不改写游戏指令。
public static class WellInterfaceProbe {
    [StructLayout(LayoutKind.Explicit, Size=176)]
    struct DebugEvent {
        [FieldOffset(0)] public uint Code;
        [FieldOffset(4)] public uint Process;
        [FieldOffset(8)] public uint Thread;
        [FieldOffset(16)] public uint ExceptionCode;
        [FieldOffset(32)] public long ExceptionAddress;
        [FieldOffset(16)] public IntPtr Handle;
        [FieldOffset(24)] public IntPtr ProcessHandle;
        [FieldOffset(32)] public IntPtr ThreadHandle;
    }
    [DllImport("kernel32", SetLastError=true)] static extern bool DebugActiveProcess(uint id);
    [DllImport("kernel32", SetLastError=true)] static extern bool DebugActiveProcessStop(uint id);
    [DllImport("kernel32", SetLastError=true)] static extern bool DebugSetProcessKillOnExit(bool kill);
    [DllImport("kernel32", SetLastError=true)] static extern bool WaitForDebugEvent(out DebugEvent e, uint ms);
    [DllImport("kernel32", SetLastError=true)] static extern bool ContinueDebugEvent(uint p,uint t,uint status);
    [DllImport("kernel32", SetLastError=true)] static extern IntPtr OpenThread(uint rights,bool inherit,uint id);
    [DllImport("kernel32", SetLastError=true)] static extern IntPtr OpenProcess(uint rights,bool inherit,uint id);
    [DllImport("kernel32", SetLastError=true)] static extern bool GetThreadContext(IntPtr t,IntPtr c);
    [DllImport("kernel32", SetLastError=true)] static extern bool SetThreadContext(IntPtr t,IntPtr c);
    [DllImport("kernel32", SetLastError=true)] static extern uint SuspendThread(IntPtr t);
    [DllImport("kernel32", SetLastError=true)] static extern uint ResumeThread(IntPtr t);
    [DllImport("kernel32", SetLastError=true)] static extern bool ReadProcessMemory(IntPtr p,IntPtr a,byte[] b,IntPtr size,out IntPtr read);
    [DllImport("kernel32")] static extern bool CloseHandle(IntPtr h);
    static readonly int[] DebugOffsets={72,80,88,96,104,112};
    static readonly long[] Rvas={0x1ab38,0x1acd5,0x1ad82,0x1abb1};
    static Dictionary<uint,long[]> originals;
    static IntPtr memory;
    static long moduleBase;
    static string logPath;
    public static Func<bool> ShouldStop = delegate { return false; };
    static void Check(bool ok,string operation) { if(!ok) throw new Win32Exception(Marshal.GetLastWin32Error(),operation); }
    static void Log(string kind,string value) {
        File.AppendAllText(logPath,DateTimeOffset.Now.ToString("o")+"\t"+kind+"\t"+value.Replace("\r"," ").Replace("\n"," ")+Environment.NewLine,Encoding.UTF8);
    }
    static byte[] Read(long address,int count) {
        byte[] b=new byte[count]; IntPtr n;
        Check(ReadProcessMemory(memory,new IntPtr(address),b,new IntPtr(count),out n) && n.ToInt64()==count,"ReadProcessMemory");return b;
    }
    static string ReadString(long address) {
        if(address==0)return "<null>";
        List<byte> bytes=new List<byte>();
        for(int i=0;i<128;i++){byte v=Read(address+i,1)[0];if(v==0)break;bytes.Add(v);}
        return Encoding.UTF8.GetString(bytes.ToArray());
    }
    static void WithContext(uint id,Action<IntPtr> action) {
        IntPtr thread=OpenThread(0x001A,false,id);
        if(thread==IntPtr.Zero)throw new Win32Exception(Marshal.GetLastWin32Error(),"OpenThread");
        IntPtr allocation=Marshal.AllocHGlobal(1248);
        IntPtr context=new IntPtr((allocation.ToInt64()+15)&~15L);
        try {
            Marshal.Copy(new byte[1232],0,context,1232);
            Marshal.WriteInt32(context,48,0x100013);
            Check(GetThreadContext(thread,context),"GetThreadContext");action(context);
            Check(SetThreadContext(thread,context),"SetThreadContext");
        }finally{Marshal.FreeHGlobal(allocation);CloseHandle(thread);}
    }
    public static bool OwnsBreakpoints(long[] slots,long control,long baseAddress) {
        if(slots==null || slots.Length!=4 || (control&0xffff00ffL)!=0x55)return false;
        for(int i=0;i<4;i++)if(slots[i]!=baseAddress+Rvas[i])return false;
        return true;
    }
    static bool OwnedContext(IntPtr c) {
        long[] slots=new long[4];for(int i=0;i<4;i++)slots[i]=Marshal.ReadInt64(c,DebugOffsets[i]);
        return OwnsBreakpoints(slots,Marshal.ReadInt64(c,112),moduleBase);
    }
    static void Arm(uint id) {
        if(originals.ContainsKey(id))return;
        WithContext(id,delegate(IntPtr c){
            bool inherited=(Marshal.ReadInt64(c,112)&255)!=0;
            if(inherited && (!OwnedContext(c) || originals.Count==0))throw new InvalidOperationException("Existing hardware breakpoints; refusing to overwrite.");
            // 新线程可能继承本监听器的寄存器，不能将这些断点保存为退出时的原值。
            long[] saved=new long[6];if(!inherited)for(int i=0;i<6;i++)saved[i]=Marshal.ReadInt64(c,DebugOffsets[i]);
            originals[id]=saved;
            for(int i=0;i<4;i++)Marshal.WriteInt64(c,DebugOffsets[i],moduleBase+Rvas[i]);
            Marshal.WriteInt64(c,104,0);Marshal.WriteInt64(c,112,0x55);
        });
    }
    static void Restore(int pid) {
        var suspended=new Dictionary<uint,IntPtr>();
        try {
            // 先暂停整组线程再恢复，避免逐个恢复时仍有线程生成继承断点的新线程。
            for(int pass=0;pass<8;pass++) {
                bool added=false;
                using(Process process=Process.GetProcessById(pid))foreach(ProcessThread thread in process.Threads) {
                    uint id=(uint)thread.Id;if(suspended.ContainsKey(id))continue;
                    IntPtr handle=OpenThread(0x001A,false,id);if(handle==IntPtr.Zero)continue;
                    if(SuspendThread(handle)==uint.MaxValue){CloseHandle(handle);continue;}
                    suspended[id]=handle;added=true;
                }
                if(!added)break;
            }
            foreach(var pair in suspended) {
                try { WithContext(pair.Key,delegate(IntPtr c){
                    if(!OwnedContext(c))return;
                    long[] saved;
                    if(!originals.TryGetValue(pair.Key,out saved))saved=new long[6];
                    for(int i=0;i<6;i++)Marshal.WriteInt64(c,DebugOffsets[i],saved[i]);
                }); } catch(Exception e){Log("restore-error","thread="+pair.Key+" "+e.Message);}
            }
        }catch(ArgumentException) { /* 游戏已退出。 */ }
        finally {foreach(var pair in suspended){ResumeThread(pair.Value);CloseHandle(pair.Value);}}
    }
    public static void Run(int pid,string directory) {
        if(IntPtr.Size!=8)throw new InvalidOperationException("Requires x64 PowerShell");
        Directory.CreateDirectory(directory);logPath=Path.Combine(directory,"interface-events.log");
        originals=new Dictionary<uint,long[]>();moduleBase=0;
        using(Process p=Process.GetProcessById(pid))foreach(ProcessModule m in p.Modules)if(m.ModuleName.Equals("GDKExtension.dll",StringComparison.OrdinalIgnoreCase))moduleBase=m.BaseAddress.ToInt64();
        if(moduleBase==0)throw new InvalidOperationException("GDKExtension.dll not loaded");
        memory=OpenProcess(0x410,false,(uint)pid);Check(memory!=IntPtr.Zero,"OpenProcess");
        bool attached=false,initial=true;
        try {
            Check(DebugActiveProcess((uint)pid),"DebugActiveProcess");attached=true;
            Check(DebugSetProcessKillOnExit(false),"DebugSetProcessKillOnExit");
            while(!ShouldStop()){
                DebugEvent e;if(!WaitForDebugEvent(out e,250)){
                    int err=Marshal.GetLastWin32Error();if(err==121)continue;throw new Win32Exception(err,"WaitForDebugEvent");
                }
                uint status=0x10002;bool exited=false;
                try {
                    if(e.Code==3){if(e.Handle!=IntPtr.Zero)CloseHandle(e.Handle);Arm(e.Thread);}
                    else if(e.Code==2)Arm(e.Thread);
                    else if(e.Code==6){if(e.Handle!=IntPtr.Zero)CloseHandle(e.Handle);}
                    else if(e.Code==4)originals.Remove(e.Thread);
                    else if(e.Code==5){Log("process-exited",pid.ToString());exited=true;attached=false;}
                    else if(e.Code==1){
                        if(e.ExceptionCode==0x80000003 && initial){initial=false;Log("armed","pid="+pid+" threads="+originals.Count);File.WriteAllText(Path.Combine(directory,"interface-ready.txt"),"ARMED pid="+pid);}
                        else if(e.ExceptionCode==0x80000004){
                            bool ours=false;
                            WithContext(e.Thread,delegate(IntPtr c){
                                long rip=Marshal.ReadInt64(c,248),dr6=Marshal.ReadInt64(c,104);
                                for(int i=0;i<4;i++)if(rip==moduleBase+Rvas[i] && (dr6&(1L<<i))!=0){
                                    ours=true;
                                    try {
                                        if(i==0)Log("achievement-request","thread="+e.Thread+" id="+ReadString(Marshal.ReadInt64(c,216))+" progress="+(uint)Marshal.ReadInt64(c,240));
                                        if(i==1)Log("achievement-rejected","thread="+e.Thread+" reason=user-not-found");
                                        if(i==2){long job=Marshal.ReadInt64(c,176);byte[] data=Read(job,36);Log("achievement-completed","thread="+e.Thread+" id="+ReadString(BitConverter.ToInt64(data,8))+" progress="+BitConverter.ToInt32(data,16)+" HRESULT=0x"+((uint)Marshal.ReadInt64(c,120)).ToString("X8"));}
                                        if(i==3)Log("achievement-rejected","thread="+e.Thread+" reason=context-creation-failed HRESULT=0x"+((uint)Marshal.ReadInt64(c,120)).ToString("X8"));
                                    }catch(Exception x){Log("decode-error",x.Message);}
                                }
                                if(ours){Marshal.WriteInt64(c,104,0);Marshal.WriteInt32(c,68,Marshal.ReadInt32(c,68)|0x10000);}
                            });
                            if(!ours)status=0x80010001;
                        }else status=0x80010001;
                    }
                }finally{Check(ContinueDebugEvent(e.Process,e.Thread,status),"ContinueDebugEvent");}
                if(exited)break;
            }
        }finally{
            if(attached){try{Restore(pid);}finally{Check(DebugActiveProcessStop((uint)pid),"DebugActiveProcessStop");}}
            if(memory!=IntPtr.Zero)CloseHandle(memory);
            Log("detached","Probe finished; game instructions unchanged");
            File.WriteAllText(Path.Combine(directory,"interface-ready.txt"),"STOPPED");
        }
    }
}
