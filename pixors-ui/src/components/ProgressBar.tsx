interface ProgressBarProps {
  percent: number
}

export function ProgressBar({ percent }: ProgressBarProps) {
  return (
    <div className="progressbar">
      <div
        className="progressbar-fill"
        style={{ width: `${percent}%` }}
      />
    </div>
  )
}
