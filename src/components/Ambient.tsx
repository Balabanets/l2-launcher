import { useMemo } from "react";

function rand(i: number, seed: number): number {
  const x = Math.sin((i + 1) * seed) * 10000;
  return x - Math.floor(x);
}

const EMBER_COUNT = 16;

/** Лёгкий атмосферный фон лаунчера: дрейфующие угли + дышащее свечение + туман. */
export function Ambient() {
  const embers = useMemo(
    () =>
      Array.from({ length: EMBER_COUNT }, (_, i) => ({
        i,
        size: 1 + rand(i, 12.9) * 2,
        left: rand(i, 78.2) * 100,
        duration: 13 + rand(i, 3.7) * 9,
        delay: -rand(i, 45.1) * 20,
        opacity: 0.14 + rand(i, 91.3) * 0.26,
        drift: (rand(i, 7.1) - 0.5) * 50,
      })),
    [],
  );

  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden>
      <div
        className="breathe absolute left-1/2 top-[-30%] h-[80%] w-[80%] -translate-x-1/2 rounded-full blur-[100px]"
        style={{ background: "radial-gradient(circle, rgba(201,164,92,0.12), transparent 70%)" }}
      />
      <div
        className="absolute bottom-[-30%] left-1/2 h-[70%] w-[100%] -translate-x-1/2 rounded-full blur-[110px]"
        style={{
          background: "radial-gradient(circle, rgba(201,164,92,0.06), transparent 70%)",
          animation: "fog-drift 42s ease-in-out infinite",
        }}
      />
      {embers.map((e) => (
        <span
          key={e.i}
          className="absolute bottom-[-10px] rounded-full"
          style={
            {
              left: `${e.left}%`,
              width: `${e.size}px`,
              height: `${e.size}px`,
              background: "rgba(224,196,134,0.9)",
              boxShadow: "0 0 6px 1px rgba(201,164,92,0.5)",
              animation: `ember-rise ${e.duration}s linear ${e.delay}s infinite`,
              "--ember-o": e.opacity,
              "--ember-x": `${e.drift}px`,
            } as React.CSSProperties
          }
        />
      ))}
    </div>
  );
}
