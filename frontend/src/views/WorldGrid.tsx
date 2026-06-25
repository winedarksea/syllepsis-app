import { equalEarthNormalized } from './worldProjection';
import type { CSSProperties, ReactNode } from 'react';

interface Props {
  kind: 'geo' | 'image';
  width: number;
  height: number;
  zoom: number;
}

export function WorldGrid({ kind, width, height, zoom }: Props) {
  return kind === 'geo'
    ? <EarthGrid width={width} height={height} zoom={zoom} />
    : <ImageGrid width={width} height={height} zoom={zoom} />;
}

function EarthGrid({ width, height, zoom }: Omit<Props, 'kind'>) {
  const step = zoom >= 6 ? 5 : zoom >= 2.5 ? 10 : 30;
  const lines: ReactNode[] = [];
  for (let longitude = -180 + step; longitude < 180; longitude += step) {
    const points: string[] = [];
    for (let latitude = -90; latitude <= 90; latitude += 3) {
      const [x, y] = equalEarthNormalized(longitude, latitude);
      points.push(`${x * width},${y * height}`);
    }
    lines.push(<polyline key={`lon-${longitude}`} points={points.join(' ')} />);
    const [labelX, labelY] = equalEarthNormalized(longitude, 0);
    lines.push(<text key={`lon-label-${longitude}`} x={labelX * width} y={labelY * height - 4 / zoom}>{longitude}°</text>);
  }
  for (let latitude = -60; latitude <= 60; latitude += step) {
    if (latitude === 0) continue;
    const points: string[] = [];
    for (let longitude = -180; longitude <= 180; longitude += 3) {
      const [x, y] = equalEarthNormalized(longitude, latitude);
      points.push(`${x * width},${y * height}`);
    }
    lines.push(<polyline key={`lat-${latitude}`} points={points.join(' ')} />);
    const [labelX, labelY] = equalEarthNormalized(-168, latitude);
    lines.push(<text key={`lat-label-${latitude}`} x={labelX * width} y={labelY * height}>{latitude}°</text>);
  }
  return <g className="wv-coordinate-grid" style={{ '--wv-grid-zoom': zoom } as CSSProperties}>{lines}</g>;
}

function ImageGrid({ width, height, zoom }: Omit<Props, 'kind'>) {
  const step = zoom >= 6 ? 0.02 : zoom >= 2.5 ? 0.05 : 0.1;
  const lines: ReactNode[] = [];
  for (let value = step; value < 1; value += step) {
    const rounded = Number(value.toFixed(3));
    lines.push(<line key={`x-${rounded}`} x1={rounded * width} y1={0} x2={rounded * width} y2={height} />);
    lines.push(<line key={`y-${rounded}`} x1={0} y1={rounded * height} x2={width} y2={rounded * height} />);
    lines.push(<text key={`xl-${rounded}`} x={rounded * width} y={14 / zoom}>{rounded}</text>);
    lines.push(<text key={`yl-${rounded}`} x={4 / zoom} y={rounded * height}>{rounded}</text>);
  }
  return <g className="wv-coordinate-grid" style={{ '--wv-grid-zoom': zoom } as CSSProperties}>{lines}</g>;
}
