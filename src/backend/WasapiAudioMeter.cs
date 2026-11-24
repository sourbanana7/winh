using System;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace LumnAudio {
    [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
    internal class MMDeviceEnumerator {}

    [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IMMDeviceEnumerator {
        int EnumAudioEndpoints(int dataFlow, int dwStateMask, out IntPtr ppDevices);
        [PreserveSig]
        int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice ppDevice);
    }

    [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IMMDevice {
        [PreserveSig]
        int Activate(ref Guid iid, int dwClsCtx, IntPtr pActivationParams, [MarshalAs(UnmanagedType.IUnknown)] out object ppInterface);
        int OpenPropertyStore(int stgmAccess, out IntPtr ppProperties);
        [PreserveSig]
        int GetId([MarshalAs(UnmanagedType.LPWStr)] out string ppstrId);
    }

    [ComImport, Guid("C02216F6-8C67-4B5B-9D00-D008E73E0064"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IAudioMeterInformation {
        [PreserveSig]
        int GetPeakValue(out float pfPeak);
    }

    [ComImport, Guid("1CB9AD4C-DBFA-4C32-B178-C2F568A703B2"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IAudioClient {
        [PreserveSig]
        int Initialize(int ShareMode, int StreamFlags, long hnsBufferDuration, long hnsPeriodicity, IntPtr pFormat, IntPtr AudioSessionGuid);
        [PreserveSig]
        int GetBufferSize(out uint pNumBufferFrames);
        [PreserveSig]
        int GetStreamLatency(out long phnsLatency);
        [PreserveSig]
        int GetCurrentPadding(out uint pNumPaddingFrames);
        [PreserveSig]
        int IsFormatSupported(int ShareMode, IntPtr pFormat, out IntPtr ppClosestMatch);
        [PreserveSig]
        int GetMixFormat(out IntPtr ppDeviceFormat);
        [PreserveSig]
        int GetDevicePeriod(out long phnsDefaultDevicePeriod, out long phnsMinimumDevicePeriod);
        [PreserveSig]
        int Start();
        [PreserveSig]
        int Stop();
        [PreserveSig]
        int Reset();
        [PreserveSig]
        int SetEventHandle(IntPtr eventHandle);
        [PreserveSig]
        int GetService(ref Guid riid, [MarshalAs(UnmanagedType.IUnknown)] out object ppv);
    }

    [ComImport, Guid("C8ADBD64-E71E-48A0-A4DE-185C395CD317"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IAudioCaptureClient {
        [PreserveSig]
        int GetBuffer(out IntPtr ppData, out uint pNumFramesToRead, out uint pdwFlags, out ulong pu64DevicePosition, out ulong pu64QPCPosition);
        [PreserveSig]
        int ReleaseBuffer(uint NumFramesRead);
        [PreserveSig]
        int GetNextPacketSize(out uint pNumFramesInNextPacket);
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct WAVEFORMATEX {
        public ushort wFormatTag;
        public ushort nChannels;
        public uint nSamplesPerSec;
        public uint nAvgBytesPerSec;
        public ushort nBlockAlign;
        public ushort wBitsPerSample;
        public ushort cbSize;
    }

    public class Program {
        private const int CLSCTX_ALL = 23;
        private const int AUDCLNT_SHAREMODE_SHARED = 0;
        private const int AUDCLNT_STREAMFLAGS_LOOPBACK = 0x00020000;
        private const uint AUDCLNT_BUFFERFLAGS_SILENT = 0x2;
        private const int AUDCLNT_E_DEVICE_INVALIDATED = unchecked((int)0x88890004);

        private static readonly Guid IID_IAudioMeterInformation = new Guid("C02216F6-8C67-4B5B-9D00-D008E73E0064");
        private static readonly Guid IID_IAudioClient = new Guid("1CB9AD4C-DBFA-4C32-B178-C2F568A703B2");
        private static readonly Guid IID_IAudioCaptureClient = new Guid("C8ADBD64-E71E-48A0-A4DE-185C395CD317");
        private static readonly Guid SUBTYPE_IEEE_FLOAT = new Guid("00000003-0000-0010-8000-00aa00389b71");
        private static readonly Guid SUBTYPE_PCM = new Guid("00000001-0000-0010-8000-00aa00389b71");

        private static readonly double[] BassFreqs = new double[] { 50.0, 75.0, 110.0, 160.0, 220.0 };
        private static readonly double[] MidFreqs = new double[] { 350.0, 600.0, 1000.0, 1800.0, 3000.0 };
        private static readonly double[] TrebleFreqs = new double[] { 5000.0, 8000.0, 12000.0 };

        private static volatile bool running = true;
        private static volatile bool paused = false;
        private static volatile int intervalMs = 30;

        private class AudioSession : IDisposable {
            public string DeviceId;
            public IAudioClient Client;
            public IAudioCaptureClient Capture;
            public IAudioMeterInformation Meter;
            public bool UseLoopback;
            public int SampleRate;
            public int Channels;
            public int BitsPerSample;
            public int BlockAlign;
            public bool IsFloat;

            public void Dispose() {
                try {
                    if (Client != null && UseLoopback) {
                        Client.Stop();
                    }
                } catch {}
                ReleaseCom(Capture);
                ReleaseCom(Client);
                ReleaseCom(Meter);
                Capture = null;
                Client = null;
                Meter = null;
            }
        }

        private static void ReleaseCom(object obj) {
            if (obj != null && Marshal.IsComObject(obj)) {
                try { Marshal.ReleaseComObject(obj); } catch {}
            }
        }

        private static string GetDefaultDeviceId(IMMDeviceEnumerator enumerator) {
            IMMDevice dev = null;
            try {
                int hr = enumerator.GetDefaultAudioEndpoint(0, 1, out dev);
                if (hr != 0 || dev == null) return null;
                string id;
                if (dev.GetId(out id) == 0) return id;
                return null;
            } catch {
                return null;
            } finally {
                ReleaseCom(dev);
            }
        }

        private static AudioSession OpenSession(IMMDeviceEnumerator enumerator) {
            IMMDevice device = null;
            try {
                int hr = enumerator.GetDefaultAudioEndpoint(0, 1, out device);
                if (hr != 0 || device == null) return null;

                string devId;
                device.GetId(out devId);

                var session = new AudioSession();
                session.DeviceId = devId;

                object meterObj;
                Guid iidMeter = IID_IAudioMeterInformation;
                if (device.Activate(ref iidMeter, CLSCTX_ALL, IntPtr.Zero, out meterObj) == 0 && meterObj != null) {
                    session.Meter = (IAudioMeterInformation)meterObj;
                }

                object clientObj;
                Guid iidClient = IID_IAudioClient;
                if (device.Activate(ref iidClient, CLSCTX_ALL, IntPtr.Zero, out clientObj) == 0 && clientObj != null) {
                    var client = (IAudioClient)clientObj;
                    IntPtr fmtPtr = IntPtr.Zero;
                    if (client.GetMixFormat(out fmtPtr) == 0 && fmtPtr != IntPtr.Zero) {
                        try {
                            WAVEFORMATEX fmt = (WAVEFORMATEX)Marshal.PtrToStructure(fmtPtr, typeof(WAVEFORMATEX));
                            bool isFloat = fmt.wFormatTag == 3;
                            bool isPcm = fmt.wFormatTag == 1;
                            if (fmt.wFormatTag == 0xFFFE && fmt.cbSize >= 22) {
                                byte[] guidBytes = new byte[16];
                                Marshal.Copy(new IntPtr(fmtPtr.ToInt64() + 24), guidBytes, 0, 16);
                                Guid subFormat = new Guid(guidBytes);
                                isFloat = subFormat == SUBTYPE_IEEE_FLOAT;
                                isPcm = subFormat == SUBTYPE_PCM;
                            }

                            if ((isFloat && fmt.wBitsPerSample == 32) || (isPcm && (fmt.wBitsPerSample == 16 || fmt.wBitsPerSample == 24 || fmt.wBitsPerSample == 32))) {
                                int hrInit = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2000000, 0, fmtPtr, IntPtr.Zero);
                                if (hrInit == 0) {
                                    object capObj;
                                    Guid iidCap = IID_IAudioCaptureClient;
                                    if (client.GetService(ref iidCap, out capObj) == 0 && capObj != null && client.Start() == 0) {
                                        session.Client = client;
                                        session.Capture = (IAudioCaptureClient)capObj;
                                        session.UseLoopback = true;
                                        session.SampleRate = (int)fmt.nSamplesPerSec;
                                        session.Channels = Math.Max(1, (int)fmt.nChannels);
                                        session.BitsPerSample = (int)fmt.wBitsPerSample;
                                        session.BlockAlign = Math.Max(1, (int)fmt.nBlockAlign);
                                        session.IsFloat = isFloat;
                                        return session;
                                    }
                                }
                            }
                        } finally {
                            Marshal.FreeCoTaskMem(fmtPtr);
                        }
                    }
                    ReleaseCom(client);
                }

                if (session.Meter != null) {
                    session.UseLoopback = false;
                    return session;
                }

                session.Dispose();
                return null;
            } catch {
                return null;
            } finally {
                ReleaseCom(device);
            }
        }

        private static double GoertzelMagnitude(float[] samples, int writePos, int count, int sampleRate, double targetFreq, double[] hann) {
            if (targetFreq >= sampleRate * 0.45) return 0.0;
            double k = (count * targetFreq) / sampleRate;
            double omega = (2.0 * Math.PI * k) / count;
            double coeff = 2.0 * Math.Cos(omega);
            double q0 = 0.0, q1 = 0.0, q2 = 0.0;
            int len = samples.Length;
            int start = (writePos - count + len) % len;

            for (int i = 0; i < count; i++) {
                double s = samples[(start + i) % len] * hann[i];
                q0 = coeff * q1 - q2 + s;
                q2 = q1;
                q1 = q0;
            }

            double real = q1 - q2 * Math.Cos(omega);
            double imag = q2 * Math.Sin(omega);
            double mag = Math.Sqrt(real * real + imag * imag) / (count * 0.25);
            return mag;
        }

        private static double ComputeBandEnergy(float[] samples, int writePos, int count, int sampleRate, double[] freqs, double[] hann, double scale) {
            double sum = 0.0;
            int valid = 0;
            for (int i = 0; i < freqs.Length; i++) {
                if (freqs[i] < sampleRate * 0.45) {
                    sum += GoertzelMagnitude(samples, writePos, count, sampleRate, freqs[i], hann);
                    valid++;
                }
            }
            if (valid == 0) return 0.0;
            double avg = sum / valid;
            double norm = Math.Sqrt(Math.Max(0.0, avg * scale));
            return Math.Min(1.0, Math.Max(0.0, norm));
        }

        private static double ExpSmoothing(double current, double target, double attackRate, double releaseRate, double dt) {
            double rate = target > current ? attackRate : releaseRate;
            double factor = 1.0 - Math.Exp(-rate * dt);
            return current + (target - current) * factor;
        }

        private static void EmitJson(double level, double peak, double bass, double mid, double treble, bool beat, bool active, string source) {
            var ic = CultureInfo.InvariantCulture;
            var sb = new StringBuilder(160);
            sb.Append("AUDIO_JSON:{\"level\":");
            sb.Append(Math.Round(level, 4).ToString(ic));
            sb.Append(",\"peak\":");
            sb.Append(Math.Round(peak, 4).ToString(ic));
            sb.Append(",\"bass\":");
            sb.Append(Math.Round(bass, 4).ToString(ic));
            sb.Append(",\"mid\":");
            sb.Append(Math.Round(mid, 4).ToString(ic));
            sb.Append(",\"treble\":");
            sb.Append(Math.Round(treble, 4).ToString(ic));
            sb.Append(",\"beat\":");
            sb.Append(beat ? "true" : "false");
            sb.Append(",\"active\":");
            sb.Append(active ? "true" : "false");
            sb.Append(",\"source\":\"");
            sb.Append(source);
            sb.Append("\"}");
            Console.WriteLine(sb.ToString());
        }

        public static void Main(string[] args) {
            Console.OutputEncoding = Encoding.UTF8;
            int parentPid = 0;

            for (int i = 0; i < args.Length; i++) {
                if (args[i] == "--parent-pid" && i + 1 < args.Length) {
                    int.TryParse(args[i + 1], out parentPid);
                    i++;
                } else if (args[i] == "--interval-ms" && i + 1 < args.Length) {
                    int parsedInterval;
                    if (int.TryParse(args[i + 1], out parsedInterval)) {
                        intervalMs = Math.Max(6, Math.Min(100, parsedInterval));
                    }
                    i++;
                }
            }

            if (Console.IsInputRedirected) {
                var stdinThread = new Thread(delegate() {
                    try {
                        string line;
                        while ((line = Console.ReadLine()) != null) {
                            string cmd = line.Trim();
                            if (cmd.Equals("QUIT", StringComparison.OrdinalIgnoreCase)) {
                                running = false;
                                break;
                            } else if (cmd.Equals("PAUSE", StringComparison.OrdinalIgnoreCase)) {
                                paused = true;
                            } else if (cmd.Equals("RESUME", StringComparison.OrdinalIgnoreCase)) {
                                paused = false;
                            } else if (cmd.StartsWith("INTERVAL:", StringComparison.OrdinalIgnoreCase)) {
                                int nextInterval;
                                if (int.TryParse(cmd.Substring(9).Trim(), out nextInterval)) {
                                    intervalMs = Math.Max(6, Math.Min(100, nextInterval));
                                }
                            }
                        }
                    } catch {}
                    if (parentPid > 0) {
                        running = false;
                    }
                });
                stdinThread.IsBackground = true;
                stdinThread.Start();
            }

            const int RingSize = 1024;
            float[] ring = new float[RingSize];
            int writePos = 0;
            double[] hann = new double[RingSize];
            for (int i = 0; i < RingSize; i++) {
                hann[i] = 0.5 * (1.0 - Math.Cos((2.0 * Math.PI * i) / (RingSize - 1)));
            }

            byte[] byteScratch = new byte[65536];
            var enumerator = (IMMDeviceEnumerator)new MMDeviceEnumerator();
            AudioSession session = null;

            double smoothLevel = 0.0;
            double smoothPeak = 0.0;
            double smoothBass = 0.0;
            double smoothMid = 0.0;
            double smoothTreble = 0.0;
            double rollingBass = 0.05;
            long lastBeatTick = 0;
            bool wasZeroEmitted = false;
            int loopCounter = 0;

            var sw = Stopwatch.StartNew();
            long lastTicks = sw.ElapsedTicks;

            try {
                while (running) {
                    loopCounter++;

                    long currentTicks = sw.ElapsedTicks;
                    double dtSec = (double)(currentTicks - lastTicks) / Stopwatch.Frequency;
                    lastTicks = currentTicks;
                    if (dtSec < 0.001) dtSec = 0.001;
                    if (dtSec > 0.200) dtSec = (double)intervalMs / 1000.0;

                    if (parentPid > 0 && (loopCounter % 50) == 0) {
                        try {
                            Process.GetProcessById(parentPid);
                        } catch {
                            break;
                        }
                    }

                    if (paused) {
                        if (session != null) {
                            session.Dispose();
                            session = null;
                        }
                        smoothLevel = 0.0;
                        smoothPeak = 0.0;
                        smoothBass = 0.0;
                        smoothMid = 0.0;
                        smoothTreble = 0.0;
                        Array.Clear(ring, 0, RingSize);
                        if (!wasZeroEmitted) {
                            EmitJson(0, 0, 0, 0, 0, false, false, "paused");
                            wasZeroEmitted = true;
                        }
                        Thread.Sleep(200);
                        lastTicks = sw.ElapsedTicks;
                        continue;
                    }

                    if (session == null) {
                        session = OpenSession(enumerator);
                        if (session == null) {
                            if (!wasZeroEmitted) {
                                EmitJson(0, 0, 0, 0, 0, false, false, "none");
                                wasZeroEmitted = true;
                            }
                            Thread.Sleep(750);
                            lastTicks = sw.ElapsedTicks;
                            continue;
                        }
                    } else if ((loopCounter % 65) == 0) {
                        string currentDevId = GetDefaultDeviceId(enumerator);
                        if (currentDevId != null && currentDevId != session.DeviceId) {
                            session.Dispose();
                            session = null;
                            Array.Clear(ring, 0, RingSize);
                            continue;
                        }
                    }

                    double instLevel = 0.0;
                    double instPeak = 0.0;
                    double instBass = 0.0;
                    double instMid = 0.0;
                    double instTreble = 0.0;
                    bool deviceLost = false;
                    string sourceName = session.UseLoopback ? "wasapi-loopback" : "wasapi-meter";

                    if (session.UseLoopback) {
                        double sumSq = 0.0;
                        int frameSamples = 0;
                        uint packetSize = 0;

                        while (true) {
                            int hrNext = session.Capture.GetNextPacketSize(out packetSize);
                            if (hrNext == AUDCLNT_E_DEVICE_INVALIDATED || hrNext < 0) {
                                deviceLost = true;
                                break;
                            }
                            if (packetSize == 0) break;

                            IntPtr dataPtr;
                            uint numFrames;
                            uint flags;
                            ulong devPos;
                            ulong qpcPos;
                            int hrBuf = session.Capture.GetBuffer(out dataPtr, out numFrames, out flags, out devPos, out qpcPos);
                            if (hrBuf == AUDCLNT_E_DEVICE_INVALIDATED || hrBuf < 0) {
                                deviceLost = true;
                                break;
                            }

                            try {
                                int totalBytes = (int)numFrames * session.BlockAlign;
                                bool silent = (flags & AUDCLNT_BUFFERFLAGS_SILENT) != 0;

                                if (!silent && totalBytes > 0 && dataPtr != IntPtr.Zero) {
                                    if (byteScratch.Length < totalBytes) {
                                        byteScratch = new byte[totalBytes];
                                    }
                                    Marshal.Copy(dataPtr, byteScratch, 0, totalBytes);

                                    int channels = session.Channels;
                                    int bytesPerSample = session.BitsPerSample / 8;
                                    for (int f = 0; f < (int)numFrames; f++) {
                                        int frameOffset = f * session.BlockAlign;
                                        double mono = 0.0;
                                        for (int c = 0; c < channels; c++) {
                                            int offset = frameOffset + c * bytesPerSample;
                                            if (offset + bytesPerSample > totalBytes) break;
                                            double sample = 0.0;
                                            if (session.IsFloat) {
                                                sample = BitConverter.ToSingle(byteScratch, offset);
                                            } else if (session.BitsPerSample == 16) {
                                                sample = BitConverter.ToInt16(byteScratch, offset) / 32768.0;
                                            } else if (session.BitsPerSample == 24) {
                                                int val = (byteScratch[offset] | (byteScratch[offset + 1] << 8) | ((sbyte)byteScratch[offset + 2] << 16));
                                                sample = val / 8388608.0;
                                            } else if (session.BitsPerSample == 32) {
                                                sample = BitConverter.ToInt32(byteScratch, offset) / 2147483648.0;
                                            }
                                            mono += sample;
                                        }
                                        mono /= channels;
                                        if (double.IsNaN(mono) || double.IsInfinity(mono)) mono = 0.0;
                                        double absMono = Math.Abs(mono);
                                        if (absMono > instPeak) instPeak = absMono;
                                        sumSq += mono * mono;
                                        ring[writePos] = (float)mono;
                                        writePos = (writePos + 1) % RingSize;
                                        frameSamples++;
                                    }
                                } else {
                                    for (int f = 0; f < (int)numFrames; f++) {
                                        ring[writePos] = 0f;
                                        writePos = (writePos + 1) % RingSize;
                                        frameSamples++;
                                    }
                                }
                            } finally {
                                session.Capture.ReleaseBuffer(numFrames);
                            }
                        }

                        if (deviceLost) {
                            session.Dispose();
                            session = null;
                            continue;
                        }

                        if (frameSamples > 0) {
                            double rms = Math.Sqrt(sumSq / frameSamples);
                            instLevel = Math.Min(1.0, Math.Sqrt(rms * 3.2));
                            instPeak = Math.Min(1.0, instPeak);
                        }

                        if (instPeak > 0.0005 || smoothLevel > 0.01) {
                            instBass = ComputeBandEnergy(ring, writePos, RingSize, session.SampleRate, BassFreqs, hann, 2.8);
                            instMid = ComputeBandEnergy(ring, writePos, RingSize, session.SampleRate, MidFreqs, hann, 3.6);
                            instTreble = ComputeBandEnergy(ring, writePos, RingSize, session.SampleRate, TrebleFreqs, hann, 4.5);
                        }
                    } else if (session.Meter != null) {
                        float meterPeak = 0f;
                        int hrMeter = session.Meter.GetPeakValue(out meterPeak);
                        if (hrMeter == AUDCLNT_E_DEVICE_INVALIDATED || hrMeter < 0) {
                            session.Dispose();
                            session = null;
                            continue;
                        }
                        instPeak = Math.Min(1.0, Math.Max(0.0, meterPeak));
                        instLevel = Math.Min(1.0, Math.Sqrt(instPeak * 1.4));
                        instBass = instLevel * 0.9;
                        instMid = instLevel * 0.75;
                        instTreble = instLevel * 0.5;
                    }

                    smoothLevel = ExpSmoothing(smoothLevel, instLevel, 35.0, 7.44, dtSec);
                    smoothPeak = instPeak > smoothPeak ? instPeak : ExpSmoothing(smoothPeak, instPeak, 0.0, 8.28, dtSec);
                    smoothBass = ExpSmoothing(smoothBass, instBass, 40.1, 8.28, dtSec);
                    smoothMid = ExpSmoothing(smoothMid, instMid, 35.0, 7.44, dtSec);
                    smoothTreble = ExpSmoothing(smoothTreble, instTreble, 35.0, 7.44, dtSec);

                    if (smoothLevel < 0.003) smoothLevel = 0.0;
                    if (smoothPeak < 0.003) smoothPeak = 0.0;
                    if (smoothBass < 0.003) smoothBass = 0.0;
                    if (smoothMid < 0.003) smoothMid = 0.0;
                    if (smoothTreble < 0.003) smoothTreble = 0.0;

                    long nowTick = Environment.TickCount & 0x7FFFFFFF;
                    bool beat = false;
                    if (instBass > 0.14 && instBass > (rollingBass * 1.35) && (nowTick - lastBeatTick) >= 140) {
                        beat = true;
                        lastBeatTick = nowTick;
                    }
                    rollingBass = ExpSmoothing(rollingBass, instBass, 2.78, 2.78, dtSec);
                    if (rollingBass < 0.04) rollingBass = 0.04;

                    bool isZero = (smoothLevel <= 0.0 && smoothPeak <= 0.0 && smoothBass <= 0.0);
                    if (!isZero) {
                        EmitJson(smoothLevel, smoothPeak, smoothBass, smoothMid, smoothTreble, beat, true, sourceName);
                        wasZeroEmitted = false;
                    } else if (!wasZeroEmitted) {
                        EmitJson(0, 0, 0, 0, 0, false, true, sourceName);
                        wasZeroEmitted = true;
                    }

                    Thread.Sleep(isZero ? Math.Max(intervalMs, 110) : intervalMs);
                }
            } finally {
                if (session != null) session.Dispose();
                ReleaseCom(enumerator);
            }
        }
    }
}
