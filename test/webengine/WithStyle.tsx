import s from "./WithStyle.module.css";

type Props = {
  message?: string;
};

function WithStyle({ message = "Hello!" }: Props) {
  return (
    <div className={s.wrapper}>
      <p>{message}</p>
    </div>
  );
}

export default WithStyle as BWEComponent<Props>;
