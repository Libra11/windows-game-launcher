using System;
using System.Diagnostics;
using System.IO;
using System.Security.Cryptography;
using System.Threading;

// 只观察指定本地游戏；启动器退出或禁用捕获后恢复调试寄存器并解除附加。
public static class ProbeHost {
    static string directory;
    static int parentId;
    static bool StopRequested() {
        try {
            using(Process p=Process.GetProcessById(parentId))if(p.HasExited)return true;
            return File.ReadAllText(Path.Combine(directory,"control.txt")).Trim()!="running";
        }catch{return true;}
    }
    public static void Main(string[] args) {
        if(args.Length!=3)return;
        string executable=Path.GetFullPath(args[0]);directory=args[1];
        if(!int.TryParse(args[2],out parentId))return;
        Directory.CreateDirectory(directory);
        string ready=Path.Combine(directory,"interface-ready.txt");
        string log=Path.Combine(directory,"interface-host.log");
        try {
            string dll=Path.Combine(Path.GetDirectoryName(executable),"GDKExtension.dll");
            using(var sha=SHA256.Create())using(var file=File.OpenRead(dll)) {
                string hash=BitConverter.ToString(sha.ComputeHash(file)).Replace("-","").ToLowerInvariant();
                if(hash!="cff3a73cb30745b31446eef51fd618790420869d119ccc8a878f404d631af033")throw new InvalidOperationException("GDK module version is not supported");
            }
            WellInterfaceProbe.ShouldStop=StopRequested;
            while(!StopRequested()) {
                File.WriteAllText(ready,"WAITING_FOR_GAME");
                Process[] games=Process.GetProcessesByName(Path.GetFileNameWithoutExtension(executable));
                foreach(Process p in games)using(p) {
                    try {
                        if(!string.Equals(p.MainModule.FileName,executable,StringComparison.OrdinalIgnoreCase))continue;
                        WellInterfaceProbe.Run(p.Id,directory);
                    }catch(Exception e){File.AppendAllText(log,DateTimeOffset.Now.ToString("o")+" "+e.Message+Environment.NewLine);File.WriteAllText(ready,"ERROR "+e.Message);Thread.Sleep(2000);}
                }
                Thread.Sleep(1000);
            }
        }catch(Exception e){File.WriteAllText(ready,"ERROR "+e.Message);return;}
        File.WriteAllText(ready,"STOPPED");
    }
}
