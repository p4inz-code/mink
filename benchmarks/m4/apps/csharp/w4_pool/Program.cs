// M4 W4 `poolrun` — C# / .NET 10 port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_155.md`.
//
// Work runs in tasks; the lock guards only the accumulator. Task scheduling
// is the .NET thread pool, not explicit OS threads — recorded as a fairness
// limitation.
using System.Globalization;
using System.Text;

namespace PoolRun;

internal static class Program
{
    private const string Usage = "usage: poolrun <squares|pipeline> <n> <workers>";

    private static void Out(string text)
    {
        Stream stdout = Console.OpenStandardOutput();
        byte[] bytes = Encoding.UTF8.GetBytes(text + "\r\n");
        stdout.Write(bytes, 0, bytes.Length);
        stdout.Flush();
    }

    private static int Fail(string label)
    {
        Stream stderr = Console.OpenStandardError();
        byte[] bytes = Encoding.UTF8.GetBytes("poolrun: " + label + "\r\n");
        stderr.Write(bytes, 0, bytes.Length);
        stderr.Flush();
        return 1;
    }

    private static (long Start, long End)[] Ranges(long n, long workers)
    {
        long chunk = n / workers;
        var parts = new (long, long)[workers];
        for (long k = 0; k < workers; k++)
        {
            long start = k * chunk + 1;
            long end = (k + 1) * chunk;
            if (k == workers - 1)
            {
                end = n;
            }
            parts[k] = (start, end);
        }
        return parts;
    }

    private static long SumSquares(long start, long end)
    {
        long total = 0;
        for (long value = start; value <= end; value++)
        {
            total += value * value;
        }
        return total;
    }

    private static long ExpectedSquares(long n) => n * (n + 1) * (2 * n + 1) / 6;

    private static int RunSquares(long n, long workers)
    {
        var parts = Ranges(n, workers);
        var subtotals = new long[workers];
        long total = 0;
        object gate = new();
        var tasks = new Task[workers];

        for (int k = 0; k < workers; k++)
        {
            int index = k;
            tasks[k] = Task.Run(() =>
            {
                long subtotal = SumSquares(parts[index].Start, parts[index].End);
                lock (gate)
                {
                    total += subtotal;
                    subtotals[index] = subtotal;
                }
            });
        }

        Out("workers=" + workers.ToString(CultureInfo.InvariantCulture));
        Out("n=" + n.ToString(CultureInfo.InvariantCulture));
        for (int k = 0; k < workers; k++)
        {
            tasks[k].Wait();
        }

        long joined = 0;
        for (int k = 0; k < workers; k++)
        {
            joined += subtotals[k];
            Out("subtotal[" + k.ToString(CultureInfo.InvariantCulture) + "]="
                + subtotals[k].ToString(CultureInfo.InvariantCulture));
        }

        long lockedTotal;
        lock (gate)
        {
            lockedTotal = total;
        }
        long expected = ExpectedSquares(n);
        Out("locked-total=" + lockedTotal.ToString(CultureInfo.InvariantCulture));
        Out("joined-total=" + joined.ToString(CultureInfo.InvariantCulture));
        Out("expected=" + expected.ToString(CultureInfo.InvariantCulture));
        return lockedTotal == expected && joined == expected ? 0 : 3;
    }

    private static int RunPipeline(long n, long workers)
    {
        var parts = Ranges(n, workers);
        var slots = new long[workers];
        var tasks = new Task[workers];

        for (int k = 0; k < workers; k++)
        {
            int index = k;
            tasks[k] = Task.Run(() =>
            {
                slots[index] = SumSquares(parts[index].Start, parts[index].End);
            });
        }

        Out("workers=" + workers.ToString(CultureInfo.InvariantCulture));
        Out("n=" + n.ToString(CultureInfo.InvariantCulture));
        Task.WaitAll(tasks); // join barrier: stage 1 completes before stage 2

        for (int k = 0; k < workers; k++)
        {
            Out("stage1[" + k.ToString(CultureInfo.InvariantCulture) + "]="
                + slots[k].ToString(CultureInfo.InvariantCulture));
        }

        long total = Task.Run(() =>
        {
            long sum = 0;
            foreach (long value in slots)
            {
                sum += value;
            }
            return sum;
        }).Result;

        long expected = ExpectedSquares(n);
        Out("total=" + total.ToString(CultureInfo.InvariantCulture));
        Out("expected=" + expected.ToString(CultureInfo.InvariantCulture));
        return total == expected ? 0 : 3;
    }

    private static int Main(string[] args)
    {
        if (args.Any(arg => arg.StartsWith('-')))
        {
            return Fail("this tool has no options");
        }
        if (args.Length != 3)
        {
            return Fail(Usage);
        }
        if (!long.TryParse(args[1], NumberStyles.None, CultureInfo.InvariantCulture,
                out long n) || n <= 0)
        {
            return Fail(Usage);
        }
        if (!long.TryParse(args[2], NumberStyles.None, CultureInfo.InvariantCulture,
                out long workers) || workers <= 0)
        {
            return Fail(Usage);
        }

        return args[0] switch
        {
            "squares" => RunSquares(n, workers),
            "pipeline" => RunPipeline(n, workers),
            _ => Fail(Usage),
        };
    }
}
