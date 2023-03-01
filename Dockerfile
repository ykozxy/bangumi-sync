FROM node
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
RUN mkdir /data
RUN ln -s /app/config /data/config
RUN mkdir cache && ln -s /app/cache /data/cache
CMD ["npm", "run", "start-server"]
