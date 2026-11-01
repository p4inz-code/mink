// M4 W1 `jsontool` — C# / .NET 10 port.
//
// Contract: `docs/benchmarks/M4/M4_SESSION_151.md` (reduced from the M1
// `validation/c_json` application).
//
// The file is read and parsed before the command is dispatched (M1 ordering),
// and any `-`-prefixed argument is a usage error because the tool has no
// options. Output is written as explicit UTF-8 bytes with CRLF so the bytes
// do not depend on console text-mode translation.
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace JsonTool;

internal static class Program
{
    private const string Usage = "usage: jsontool <validate|type|sum|compact> <file> [args]";

    private static void WriteLine(Stream stream, string text)
    {
        byte[] bytes = Encoding.UTF8.GetBytes(text + "\r\n");
        stream.Write(bytes, 0, bytes.Length);
        stream.Flush();
    }

    private static void Out(string text) => WriteLine(Console.OpenStandardOutput(), text);

    private static int Fail(string text)
    {
        WriteLine(Console.OpenStandardError(), "jsontool: " + text);
        return 1;
    }

    private static int FailFile(string label, string path)
    {
        WriteLine(Console.OpenStandardError(), "jsontool: " + label + path);
        return 2;
    }

    private static string TypeName(JsonElement element) => element.ValueKind switch
    {
        JsonValueKind.Null => "null",
        JsonValueKind.True or JsonValueKind.False => "bool",
        JsonValueKind.Number => "number",
        JsonValueKind.String => "string",
        JsonValueKind.Array => "array",
        JsonValueKind.Object => "object",
        _ => "unknown",
    };

    private static string Compact(JsonElement element)
    {
        using var buffer = new MemoryStream();
        var options = new JsonWriterOptions
        {
            Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        };
        using (var writer = new Utf8JsonWriter(buffer, options))
        {
            element.WriteTo(writer);
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    private static int Main(string[] args)
    {
        if (args.Any(arg => arg.StartsWith('-')))
        {
            return Fail("this tool has no options");
        }
        if (args.Length < 2)
        {
            return Fail(Usage);
        }

        string command = args[0];
        string path = args[1];

        if (!File.Exists(path))
        {
            return Directory.Exists(path)
                ? FailFile("not a regular file: ", path)
                : FailFile("input file does not exist: ", path);
        }

        byte[] raw;
        try
        {
            raw = File.ReadAllBytes(path);
        }
        catch (IOException)
        {
            return FailFile("input file does not exist: ", path);
        }

        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(raw);
        }
        catch (JsonException)
        {
            WriteLine(Console.OpenStandardError(), "jsontool: invalid JSON");
            return 3;
        }

        using (document)
        {
            JsonElement root = document.RootElement;

            switch (command)
            {
                case "validate":
                    if (args.Length != 2)
                    {
                        return Fail("usage: jsontool validate <file>");
                    }
                    Out("ok");
                    return 0;

                case "type":
                    if (args.Length != 2)
                    {
                        return Fail("usage: jsontool type <file>");
                    }
                    Out(TypeName(root));
                    return 0;

                case "sum":
                {
                    if (args.Length != 3)
                    {
                        return Fail("usage: jsontool sum <file> <field>");
                    }
                    if (root.ValueKind != JsonValueKind.Array)
                    {
                        WriteLine(Console.OpenStandardError(),
                            "jsontool: document is not an array");
                        return 4;
                    }
                    string field = args[2];
                    long total = 0;
                    foreach (JsonElement record in root.EnumerateArray())
                    {
                        if (record.ValueKind != JsonValueKind.Object)
                        {
                            continue;
                        }
                        if (record.TryGetProperty(field, out JsonElement value)
                            && value.ValueKind == JsonValueKind.Number
                            && value.TryGetInt64(out long number))
                        {
                            total += number;
                        }
                    }
                    Out(total.ToString(System.Globalization.CultureInfo.InvariantCulture));
                    return 0;
                }

                case "compact":
                    if (args.Length != 2)
                    {
                        return Fail("usage: jsontool compact <file>");
                    }
                    Out(Compact(root));
                    return 0;

                default:
                    return Fail(Usage);
            }
        }
    }
}
