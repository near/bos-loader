type Props = {
  message?: string;
};

function NoStyle({ message = "Hello!" }: Props) {
  return (
    <div>
      <p>{message}</p>
    </div>
  );
}

export default NoStyle as BWEComponent<Props>;
